// Corpus finding, NOT FIXED: a bodiless ambient declaration inside a `.d.ts`
// is minted as a call-index def and wins the name match, so a call edge points
// at a declaration that has no body and can never be a call target.
//
// Measured on microsoft/TypeScript @9a8581c3: 172 resolved_edge rows land in a
// `.d.ts`. 135 of them land in `tsc/internal/bundled/libs/lib.es2015.reflect.d.ts`,
// where the `Reflect.get` declaration captures 125 plain `.get(...)` calls, and
// 15 more bind `parseInt` to `lib.es5.d.ts`.
//
// Repro:
//   extract --resolve --family call caller.ts ambient.d.ts
// Expected: zero resolved_edge rows (a bodiless declaration is not a callee).
// Observed: one resolved_edge, caller.ts:read -> ambient.d.ts:get.
interface Store {
    get(key: string): unknown;
}

export function read(store: Store, key: string): unknown {
    return store.get(key);
}
