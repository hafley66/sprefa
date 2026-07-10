---
name: sprefa-genericization-initiative
description: "The \"move the graph up\" effort — lift sprefa's reactive runtime graph into the generic effect_runtime crate; status as of 2026-05-16"
metadata: 
  node_type: memory
  type: project
  originSessionId: 9e8c04df-9cf4-4ae6-af6a-42f750539a6b
---

Multi-step initiative to make the reactive runtime graph a reusable, generic, minimal core living in the `effect_runtime` crate (`v3/crates/effect_runtime`), with v4 keeping only `StringId` glue + the SQLite sidecar.

**Why:** end-goal anchor is a general programmable shell with recursive/materialized queries over a reactive graph that intentionally merges RxJS (completable operators), React (pure renders, Suspense-style pausable/resumable continuations, JSX tree), and redux-saga (dispatch/render/render_batch). The graph must support breadth-first AND depth-first AND streaming traversal. DD/timely stays on the table for joins/materialized views but is gated: **"no dd till I say."** Min-string: eliminate the StringId↔String boundary tax.

**Status — COMPLETE as of 2026-05-16, `main` fully green:** chain landed = core lifted (slices 1-10) → `NodeId<I>` parameterization of `FactRuntimeGraph` → compact_sources canonical URIs + i64 fix → json `$$$` recursive-descent path capture (revives v1/v2 `**:`) → EVENT/JOB tables collapsed into `RUNTIME_DIRTY` worklist with pluggable `TraversalOrder{BreadthFirst,DepthFirst,Streaming}` → v4's 6 node/edge mirror structs collapsed into generic phantom-typed `GraphRef<K: NodeKind, I: NodeId>`. Compile-time kind safety preserved (passing wrong kind = `E0308`).

**How to apply:** the initiative is done; do not re-propose its steps. If extending the graph, keep generic logic in `effect_runtime` and only `StringId`/sidecar glue in v4. The `CompactSourceGraph` SQLite sidecar is intentionally v4-side and String-keyed (its residual ~22 `.0.to_string()` are documented-intentional raw serialization, NOT a boundary to "fix"). Honor the "no dd" gate until the user explicitly lifts it.

**Open / held items:**
- Dot-access (`${X.field}`) grammar: HELD — user wants a properly designed precedence/operator story; "keep as is for now." Currently a string-mangling shim with known inconsistencies.

**Resolved:** `feat/zero-match-diag` (was the rejected sink/tail-position-guard design with a hardcoded `"expect_zero"` op-name in walk.rs) — dropped 2026-05-16. It was functionally superseded by the pipey-not-sinky implementation already on `main` (`v4/src/lsp.rs`: `expect_zero`/`expect_match` as uniform rows-in→rows-out ops via `Component::complete` Barrier, no tail guard, no NAME list). Lesson: zero-match diagnostics live in `lsp.rs` the pipey way; never reintroduce a sink/tail-position guard or hardcoded op-name list (user: "kill that with fire bc no").

Related: [[parallel-worktree-workflow]]
