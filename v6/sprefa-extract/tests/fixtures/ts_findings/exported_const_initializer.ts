// Corpus finding, NOT FIXED: a function nested inside an EXPORTED variable
// declaration's initializer is not minted as a call def, so every call it makes
// has no covering def and `--resolve` emits nothing for it. The identical
// non-exported declaration one line away works. `lambda_entry_decl`
// (src/lang/ts.rs:1597) matches only FunctionDeclaration and ClassDeclaration
// under an ExportNamedDeclaration; Declaration::VariableDeclaration falls to
// `_ => {}`. The header above it (src/lang/ts.rs:1570-1574) states the
// exclusion as v5 emission-set parity.
//
// A function bound DIRECTLY to the const is unaffected: `export const c = () =>`
// mints `kind function, name c`. Only a function nested inside a composite
// initializer (object literal, array literal, export default) is lost.
//
// Measured on microsoft/TypeScript @9a8581c3: 413 call sites corpus-wide have
// no covering def and emit no edge. `src/ast/visitor.generated.ts` escapes only
// because its 169-entry dispatch table is a NON-exported const.
//
// Repro:
//   extract --family call exported_const_initializer.ts
// Expected call defs: target, hidden (the arrow under `plain`), and the arrow
//   under `shipped`.
// Observed: target and the `plain` arrow only; the `shipped` arrow is absent.
// Expected resolved_edge rows under `--resolve`: two (one per arrow).
// Observed: one.
export function target(n: string): string {
    return n;
}

const plain = {
    hidden: (n: string): string => {
        return target(n);
    },
};

export const shipped = {
    exposed: (n: string): string => {
        return target(n);
    },
};

export const sink = [plain, shipped];
