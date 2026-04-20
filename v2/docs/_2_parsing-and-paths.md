# Parsing direction and path concepts

Source: chat 2026-04-18. Mostly speculative; included for posterity of
the design conversation. Author preference for tree-sitter direction is
[tentative].

## Parser direction

Sprf currently hand-rolls its parser in `crates/sprf-lsp` and
`v2/src/_8_parse.rs`. The design conversation surveyed fault-tolerant
parsing options and leaned toward tree-sitter:

- IDE-grade error tolerance (ERROR/MISSING nodes) for free.
- Incremental reparse via `Tree::edit` + `Parser::parse(text, Some(&old))`.
- One grammar reusable by sprf parser, ast-grep, and editor plugins
  (Helix/Neovim/Zed) via published `tree-sitter-sprefa`.
- Language injection (`Parser::set_included_ranges`) gives the
  embeddable-grammar pattern: each op's body parsed by its own grammar.

Library candidates surveyed are catalogued in
`~/projects/claude-research/commands/parsing/` (tree-sitter, chumsky,
rowan-ungrammar, lalrpop, pest, winnow, nom, peg, lezer, comparison.md).

Status: not committed. The hand-written parser is functional. Move to
tree-sitter is a future direction that buys editor-grade tolerance and
single-grammar reuse if the maintenance trade is worth it.

## Two path namespaces

The design discussion distinguishes two paths:

- **Static path** — position in source. Walked off the parse tree at
  parse time. Stable across runs. Anchors diagnostics.
- **Dynamic path** — breadcrumb a cursor accumulates as it flows through
  the running pipeline. Today's `SprfPath`. Per-cursor, per-tick.
  Framework-owned, leaf-first, runner-tagged (per
  `project_v2_path_tagging` memory).

The static path is derivable from the parse tree; the dynamic path is
minted by the runner. They have a 1:N relation: one static node, many
cursor visits at runtime.

## Path addressing language [tentative]

A `/`-delimited segment language inside `&{...}` was floated as the
addressing surface:

```
&{.fs}                        cursor slot
&{/grep/ast}                  absolute static path from rule root
&{/grep/fork[rust]/ast}       path with predicate
```

Compiles to a tree-sitter Query as IR if the parser moves to tree-sitter.
None of this is locked; it is the direction the conversation pointed.

## Parse-time diagnostics layering

If/when the parser can produce error nodes structurally, diagnostics fall
out in layers:

1. **Structural** — tree-sitter `is_error()` / `is_missing()` walk.
2. **Semantic per-op** — each op's `validate(&op_call)` slot reports its
   own arg/body issues. Mirrors the existing op-owns-everything rule.
3. **Capture flow** — bind/use analysis on the term lattice (see
   `_0_term-language.md`). Unbound refs, shadowed bindings, wrong-position
   refs.
4. **Path resolution** — each path literal resolved against the static
   path index. Zero matches → error with the partial prefix that did
   match.
5. **Injection errors** — sub-lang errors rebased into the body byte
   range and surfaced as host diagnostics.

All five layers run on every `did_change`. None of this is implemented;
this is the design shape the discussion converged on.
