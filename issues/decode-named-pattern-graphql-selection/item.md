---
created: 2026-08-22
updated: 2026-08-22
type: feature
reporter: chris
assignee: chris
status: open
priority: normal
epic: cheap-fast-analysis
---

# Named decode patterns, and a GraphQL selection emitted from the pattern

## The woe (ghcache.dl6, 2026-08-22)

The GraphQL selection set is written twice: as a 14-line string spliced by
`concat` (`v6/dl/ghcache/ghcache.dl6:843-857`) and as the decode pattern over
the answer (`:928-937`, plus five fan-out rules). Nothing checks the two agree.

## The idea (user: after strings are done, not before)

1. A decode pattern can be declared once under a name and spliced into any
   `decode(...)` by that name. Today patterns are inline only.
2. `graphql_selection(<pattern>, args)` emits the selection text from the
   pattern: keys become fields, nested braces nest, `[... ]` spreads vanish,
   captures and types drop. Per-repo aliasing (`repo_N: repository(owner, name)`)
   stays data from `pr_batch_member`. Argument lists (`first: 100`) live in
   the emitter's args. Union fragments (`... on StatusContext`) have no pattern
   form and stay text, which is the reason to weigh this before building.

Alternative that needs no syntax: a compile-time check that every key a decode
captures appears in the selection string.
