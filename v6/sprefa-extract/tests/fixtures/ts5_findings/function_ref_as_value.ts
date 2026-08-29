// Corpus finding, NOT FIXED: a function named as a value, never called at that
// spot, produces no `site` row and therefore no resolved_edge. TypeScript 5.9
// builds its whole transform pipeline this way, `transformers.push(transformX)`
// in `src/compiler/transformer.ts`; five of the ten largest unreachable defs in
// the entrypoint crawl are those transformers, each with 0 call sites in src/**.
//
// Repro: extract --family call function_ref_as_value.ts
// Expected: an edge a call-graph crawl can follow from `register` to `handler`.
// Observed: the only `site` row in `register` is `push`. The reference is in
// the stream, as a df `node` kind=var_read name=handler at the argument span,
// and neither the call plane nor --resolve joins it, so `handler` is
// unreachable from every entrypoint.

export function handler(n: number): number {
    return n * 2;
}

export function register(table: ((n: number) => number)[]): void {
    table.push(handler);
}
