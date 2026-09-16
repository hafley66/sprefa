# Type annotation surface and elaboration

Implement the first card from `plans/2026-08-19-applicative-type-annotations.md`.

Scope:

- Parse `@(Type, [Applications...])` wherever a type expression is accepted.
- Preserve a dedicated AST until module and generic resolution establish the owning member and concrete type.
- Print canonically and update tree-sitter CST/node types.
- Elaborate applications left to right with implicit `Target`, but keep compiler execution and key evidence consumption outside this card.
- Add named syntax/signature-shape refusals only where this card owns enough information.
- Add focused parse/print/reparse and CST CI.
- Read current golden and compiler phase order before editing.
- Commit with `Refs-Issue: @type-annotation-surface`.
- Run `boop tell-parent --kind completion --body "type-annotation-surface done commit=<sha>"` after CI.

Do not alter existing runtime key behavior in this card.
