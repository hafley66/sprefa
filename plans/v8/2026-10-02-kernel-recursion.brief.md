# Brief: no recursion in kernel, eval and IR-lowering Rust

User (2026-10-02): recursion in the kernel in Rust is not okay. List: `2026-10-02-two-engines.recursion.txt`
(ryii syntax resolve, lower bound; file paths predate the engine deletion and the
`_7_execute.rs` -> `_7_goals.rs` move). Scope: `src/_6_eval/`, `src/_2_lower/` (IR and goal
lowering), `src/_9_runtime/`, sqlite_ivm `crates/ivm-*`.

## Goal

- Every self-calling or mutually recursive function in scope walks with an explicit stack or
  worklist and a named limit diagnostic (keep existing depth limits as named constants).
- Find cycles too: `ryii src --resolve --kinds cst,call --sqlite <db>` then a recursive CTE over
  `resolved_edge`; build a SCIP index (`rust-analyzer scip . --output index.scip`) and use
  `ryii slow` when the syntax resolve misses method calls.
- A test that runs that query (or an equivalent) and fails on any self-call or call cycle in scope.

Method, laws, validation, receipt: as `plans/v8/2026-10-02-strings.brief.md`.
