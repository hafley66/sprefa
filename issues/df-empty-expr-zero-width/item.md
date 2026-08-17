---
created: 2026-08-15
updated: 2026-08-15
type: bug
status: fixed
priority: normal
labels:
- extract
- df
- size:small
- area:extract
- pkg:extract
- bugmine
closed: 2026-08-15
commits:
- hash: da32b083
  summary: extract-empty-statements-stop-minting-zero-width-df-expr-nodes
---

# df extracts zero-width 0:0 expr nodes for empty statements

_Source: v6/sprefa-extract/src/lang/rust.rs:1958_

## Description

Per-file df invariant sweep over external-scale Rust corpora (tokio + rust-analyzer, --family df) reports nonzero zero-width df node violations, all one root cause: an empty Rust statement (bare `;`) resolves to a 0:0 expr node.

The `_ =>` fallback arm at v6/sprefa-extract/src/lang/rust.rs:1958 mints DfNodeKind::Expr with node_span; when the syn node has no expression (empty statement), the span resolves to (0,0), producing:
- a zero-width df node (invariant 2), and
- duplicate (span,kind) node keys at (0,0,'expr') (invariant 3), because every empty statement in a file collapses to the same placeholder key.

Smallest offending file: rust-analyzer/crates/parser/test_data/parser/inline/ok/nocontentexpr.rs (50 bytes; source `fn foo(){ ;;;some_expr();;;;{;;;};;;;Ok(()) }`) emits 12 copies of (0,0,'expr'). A second hit: tokio/tests-build/tests/fail/macros_type_mismatch.rs (1874 bytes) emits one (0,0,'expr').

Tally over the corpus (invariant 1 span, 2 zero-width, 3 dup, 4 edge, 5 process): tokio 793 files / 205205 facts / 0,1,0,0,0; rust-analyzer 1478 files / 1073493 facts / 0,26,3,0,0. Invariants 1, 4, 5 are clean. Only empty-statement expr spans are affected.

No edges reference these placeholder nodes (invariant 4 clean), so this is an identity/span hygiene defect, not a flow-correctness break. Decide: suppress the placeholder, or give empty statements a real (zero-length at the statement) span.
