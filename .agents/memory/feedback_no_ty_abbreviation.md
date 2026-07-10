---
name: feedback-no-ty-abbreviation
description: "No `ty` abbreviation on the dl language surface — column/var name is `type` (works fine, no keyword collision); Rust internals keep ty (Rust keyword)"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: ab4a16de-586c-4508-93c0-71986c34bbfa
---

Chris (2026-07-10): "can we not use shortened type to ty, just say type tho plz."

**Why:** `type` works as a dl column and variable name — the `type` decl keyword only
dispatches at item position (verified live). The `ty` habit came from Rust, where
`type` IS reserved; the DSL never needed it.

**How to apply:** any new rel/column/var on the dl surface spells `type` out
(rel_col, type_decl_row, _shapes all renamed in 1176f01 on feat/type-decl-row;
diag code shape-unknown-ty -> shape-unknown-type). Rust-internal identifiers keep
`ty`. Same instinct as [[feedback-descriptive-dl-var-names]] and the no-casual-codenames
rule: the surface says the actual word. Also flagged: `ts` is ambiguous in this repo
(TypeScript in lang slugs, tree-sitter in engine code like run_ts) — spell out
"typescript" in any new user-facing surface.
