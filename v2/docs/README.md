# v2 design docs

Conversation captures. Confirmed direction is in plain text; speculative
bits are marked `[tentative]` so future readers can tell what was locked
from what was floated.

- `_0_term-language.md` — term/decl/ref lattice; `${X}` / `$$${X}` /
  `%{X}` sigil family; brace rule; `$` outward / `&` inward direction.
- `_1_ast-grep-extension.md` — sprf surface as compositional extension
  of ast-grep's YAML rule language; `op[lang](<P>) { ... }` block shape;
  capture lift across the host/sub-lang boundary.
- `_2_parsing-and-paths.md` — tree-sitter parser direction; static vs
  dynamic path namespaces; parse-time diagnostic layering.
- `_3_influences.md` — bash / css / prolog / ast-grep / rxjs / sql /
  react / redux-sagas adjacencies; author-confirmed set.
- `_4_lofty-goals.md` — direction-of-travel quote: tree-as-comptime,
  comment-as-carrier syntax, cross-repo hover overlays.
- `_5_tree-sitter-direction.md` — author-confirmed: tree-sitter, with
  per-op grammars, host grammar on top. Maps to existing `Operator`
  trait slots (`bracket_grammar`, `paren_grammar`, `brace_mode`).
  Lists open questions (host grammar scope, BraceMode disposition,
  walker DSL, migration order, term-language sigil churn).
- `_6_lowering-pipeline.md` — five-stage pipeline (host-parse,
  body-extract, body-inject, lower, run); PHP analogy for
  interpolation; `set_included_ranges` with discontinuous holes;
  pleasant op-authoring criteria; six open lowering questions.
- `_7_lsp-as-op.md` — [tentative] LSP as sprf-authored ops; server
  becomes a thin dispatcher keyed on (capability, node kind);
  cursor as the universal LSP response shape; three-layer separation
  (parse / runtime / programmability).
- `_8_string-redirection.md` — [tentative] bashism queue: `>>{slot}`
  redirect direction, `${[expr]}` inline expression evaluation,
  promoting `&{...}` to "cursor query language." Backburner until
  layers 1 and 2 stabilize.

These supplement (do not replace) the project-vision and runtime memory
in `~/.claude/projects/-Users-chrishafley-projects-sprefa/memory/`.
