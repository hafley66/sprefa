// Corpus finding, NOT FIXED: the `node` record carries no exported flag, so a
// module-private def and an exported def of the same name are indistinguishable
// in the def index and the name stays ambiguous. In TypeScript 5.9,
// `src/compiler/parser.ts:2318 function isIdentifier()` is module-private and
// `src/compiler/factory/nodeTests.ts:318 export function isIdentifier` is the
// one every caller means; `isIdentifier` has 465 call sites in src/** and 24
// resolved_edge rows, all of them same-file. 2399 ambiguous sites in that
// corpus stay ambiguous for this reason after the barrel closure is applied.
//
// Repro: extract --resolve --family call private_shadows_export/*.ts
// Expected: caller `check` -> `nodeTests.ts:isIdentifier`, the only export.
// Observed: no such edge. The only row is the same-file `parse` ->
// `parser.ts:isIdentifier`, which a same-file tie-break already handles; the
// cross-file import that names one file is the one dropped.

export function isIdentifier(kind: number): boolean {
    return kind === 80;
}
