// Corpus finding, NOT FIXED: the `site` span covers the whole callee expression
// including the receiver, and the name match reads only the last segment, so a
// `Array.prototype.push` call binds to any module function spelled `push`.
// Over TypeScript 5.9's src/**, 3175 of 75089 edges (4.2%) have this shape;
// `src/compiler/tracing.ts:push` alone captures 2064 array pushes, and
// `src/compiler/binder.ts:bind` captures 54 `fn.bind(...)` calls.
//
// Repro: extract --resolve --family call receiver_blind_prototype.ts tracing_like.ts
// Expected: no edge out of `collect`; `out` is an Array, not the tracing module.
// Observed: one resolved_edge, caller `collect`, callee `tracing_like.ts:push`.
// The `site` row proves the receiver was read: its span text is `out.push`.

export function collect(xs: number[]): number[] {
    const out: number[] = [];
    for (const x of xs) {
        out.push(x);
    }
    return out;
}
