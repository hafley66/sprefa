// Corpus finding, NOT FIXED: --resolve names a lambda caller `closure@<byte
// offset>` while `--family call` names every lambda `node` null, so the two
// planes share no key and a BFS cannot pass through a closure. 17592 of the
// 75089 edges over TypeScript 5.9's src/** (23.4%) carry such a caller, across
// 6476 distinct closures. Refolding each one onto its nearest enclosing named
// def raises entrypoint reachability from 5854 to 7344 of 14047 defs.
//
// Repro: extract --resolve --family call closure_caller_key.ts closure_callee.ts
//        extract --family call closure_caller_key.ts
// Expected: a caller key present in both streams.
// Observed: resolved_edge caller_name is `closure@<n>`; the covering `node` row
// for that span is kind=lambda name=null. `outer` never appears as a caller.

import { leaf } from "./closure_callee.js";

export function outer(xs: number[]): number[] {
    return xs.map(x => leaf(x));
}
