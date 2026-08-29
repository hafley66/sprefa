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
export function report(): void {
  console.log("not the local log");
}
