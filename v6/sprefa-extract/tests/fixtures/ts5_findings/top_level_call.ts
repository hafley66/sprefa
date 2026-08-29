// Corpus finding, NOT FIXED: a call site in a top-level statement has no
// covering `node`, and every resolved_edge carries a non-null caller_name, so
// the site is dropped (`resolve_calls`, src/lang/ts.rs:3383). 1358 sites in
// TypeScript 5.9's src/** have no covering def, 740 of them naming a callee
// with exactly one def in the universe. `src/tsc/tsc.ts` is eight such sites
// and zero edges: the compiler's own entrypoint reaches nothing.
//
// Repro: extract --resolve --family call top_level_call.ts top_level_callee.ts
// Expected: three resolved_edge rows, one per call to `entry`.
// Observed: one, from `insideFn`. `bootExpr` and `bootStmt` yield none.

import { entry } from "./top_level_callee.js";

export const bootExpr = entry(1);

entry(2);

export function insideFn(): number {
    return entry(3);
}
