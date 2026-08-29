// Corpus finding, NOT FIXED: a member call keeps only its property name, so
// `console.log(...)` here carries callee "log" with the receiver dropped
// (`callee_name`, ts.rs `E::StaticMemberExpression` arm). Paired with
// `corpus_2_logger.ts` under `--resolve`, the name match then emits a
// resolved_edge from `report` to that file's free `log`, which this file
// neither imports nor calls.
//
// Expected: no resolved_edge out of `report`, because `console` is not
// `corpus_2_logger.ts`.
// Observed: one resolved_edge, kind name_resolve, callee_name "log".
//
// WHY THIS SITS OUTSIDE tests/fixtures/ts. That directory is the scip-ratchet
// corpus (golden_parity.rs `call_resolve_scip_ratchet_ts`), which asserts
// overbound == 0. This pair IS an overbind, so it can never go green there.
// Its `console` is also untyped under that root's `"lib": ["es2020"]`
// (tests/fixtures/ts/tsconfig.json), so scip emits no occurrence for the site.
export function report(): void {
  console.log("not the local log");
}
