# Lofty goals

Source: chat 2026-04-18, author quote. Aspirational. The author labelled
this "lofty lofty" — preserve here as direction-of-travel, not roadmap.

> "my lofty goals are taking any tree or tree-projection-of-graph like
> zig takes comptime and being able to typespec render codegen in an
> auto area of comment syntax or 2 comment scope targeting so abusing
> comments for transluding or targeting or adding notes to lsp hover
> diags across repos to say 'this is a pointer to XYZ and we have
> links' bc being able to program unique patterns across all trees of
> code in a microservices/multi-repo env, as easy as possible.
> lofty lofty"

## Components of the goal (author-stated)

- Take any tree or tree-projection-of-graph as input. Zig comptime is
  the spiritual reference: a code object you can compute against.
- Typespec, render, codegen — three things you can do once you hold
  the tree.
- Comments as the carrier syntax. Either an "auto area" inside a
  single comment, or "2 comment scope targeting" (a pair of comments
  bracketing a region).
- Comments used to:
  - transclude (pull content from elsewhere into the rendered view)
  - target (mark a region as the subject of a sprf operation)
  - add notes to LSP hover/diag output
- Cross-repo: hover and diag overlays surface notes that originated in
  a different repo. "this is a pointer to XYZ and we have links."
- Goal-of-goals: program unique patterns across every tree of code in
  a microservices / multi-repo environment, as easy as possible.

## Existing partial fragments

These memory entries point at pieces of the lofty goal that already
have a planned shape:

- `project_md_marker_tags` — `marker()` op for comment-bounded
  extraction; `md()` for markdown pattern matching; doc sync. The
  comment-as-carrier mechanism.
- `project_vision` — cross-codebase causal linking engine; rename is
  one application. The cross-repo backbone.
- `project_dogfood_vision` — reactive runtime, note ops, assumption
  drift, circuit-diagram render, type-graph cognition.
- `project_type_graph_layer` — static type/property tree extraction
  for rename propagation through type hierarchies. The
  tree-projection-of-graph piece.
- `project_lsp_hover_ux` — hover shows file://#L links, markdown
  table of emitted rows, config-context banner. The hover overlay
  surface.
- `project_scan_pointer_runtime` — scan pointers, Tri verified,
  assumption checker, cross-repo edge thesis. The "pointer to XYZ
  with links" backbone.

## Reader inference (not author statement)

The pieces line up roughly as:

```
comment-as-carrier  →  marker()/md() ops
tree-projection     →  type-graph layer + ast-grep super-grammar
cross-repo overlay  →  scan pointers + result store + LSP hover
codegen             →  effect-runtime split (writes as deferred effects)
```

Whether this composition matches the author's actual mental model is
not confirmed. It is one consistent reading of the existing memory
entries against the lofty-goals quote.
