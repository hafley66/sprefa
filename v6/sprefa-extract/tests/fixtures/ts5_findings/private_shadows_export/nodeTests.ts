// Corpus finding, FIXED by the ts module plane. The `node` record still carries
// no exported flag and no longer needs one: the importer's BINDING picks the
// file, so a module-private def cannot shadow an imported one. In TypeScript
// 5.9 `src/compiler/parser.ts:2318 function isIdentifier()` is module-private
// and `src/compiler/factory/nodeTests.ts:318 export function isIdentifier` is
// the one every caller means.
//
// Repro: extract --resolve --family call private_shadows_export/*.ts
// Expected and observed: `check` -> `nodeTests.ts:isIdentifier`
// (import_resolve) AND `parse` -> `parser.ts:isIdentifier` (name_resolve, the
// same-file one). The graded version of this shape is
// tests/54_ts_module_plane.rs.

export function isIdentifier(kind: number): boolean {
    return kind === 80;
}
