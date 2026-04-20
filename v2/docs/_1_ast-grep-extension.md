# Sprf as ast-grep compositional extension

Source: chat 2026-04-18. Some bits confirmed by user, some [tentative]
suggestions from the design discussion.

## Thesis (confirmed)

Sprf does not replace ast-grep. The sprf surface gives ast-grep what its
YAML rule format can express but its inline pattern grammar cannot:
composable rule trees, written as readable pipe blocks. Ops lower to
ast-grep's existing Rust API.

The runtime adds the parts ast-grep does not have: multi-repo execution,
revisions, term bindings, addressing, the result store, the LSP.

## Op shape (confirmed direction)

```
ast[lang]                              # context only
ast[lang](<pattern>)                   # pattern only
ast[lang](<pattern>) {                 # pattern with sub-rule block
  > ast[lang](<sub>)
}
```

`[lang]` selects the tree-sitter grammar. Lowers to ast-grep's `language:`
field. Multiple lang variants share the same lowering machinery; only the
language tag differs.

## Block operators

| Operator    | Meaning                                            | Status     |
| ----------- | -------------------------------------------------- | ---------- |
| `>`         | pipe / sequence (already in v2.1 grammar)          | confirmed  |
| `;`         | fork / distributes parent (already in v2.1)        | confirmed  |
| `#`         | comment (already in v2.1)                          | confirmed  |
| `!` prefix  | negation on inner op                               | [tentative] |
| `!` standalone | "done collecting, no longer cartesian, take everything and go do stuff" — pipeline-level barrier between matching phase and downstream consumption. Exact semantics undecided; possibly used for rendering / effect handoff. | reserved, semantics open |

## Lowering hints (tentative)

The mapping from sprf op blocks to ast-grep YAML rule combinators is the
direction the design is heading. Concrete table is not locked.

Plausible shape:

| Sprf                                              | ast-grep YAML                              |
| ------------------------------------------------- | ------------------------------------------ |
| `ast[rs](<P>)`                                    | `{ pattern: P, language: rust }`           |
| `ast[rs](<P>) { > ast[rs](<Q>) }`                 | `{ pattern: P, has: { pattern: Q } }`      |
| `ast[rs](<P>) { > A > B }`                        | `{ pattern: P, all: [A, B] }`              |
| `ast[rs](<P>) { > A ; B }`                        | `{ pattern: P, any: [A, B] }`              |

Relational keywords (`inside`, `has`, `follows`, `precedes`, `matches`)
from ast-grep's YAML rule schema are candidates for surfacing as block
prefixes. Not locked.

## Pattern body delimiter

ast-grep patterns can contain arbitrary source. The host needs a delimiter
that lets the pattern carry generics, parens, braces without escaping.

Discussion floated `<<...>>` (doubled angle) as a candidate. Not locked.

## Sigil safety resolution

The earlier `g{}` / `re{}` host-sigil idea is retired. Pattern primitives
(glob, regex, ast-grep pattern body) are op args, not host sigils. Capture
stays as the standard `${X}` binding applied at op-result level.

This keeps the host sigil family small: `$`, `&`, `%`. Pattern languages
live as op-arg keys (e.g. `glob = "..."`, `re = "..."`, body
`<<...>>`). [tentative on the exact arg-key spelling]

## ast-grep capture lift

When an op runs and matches, its sub-lang metavars enter the host binding
namespace. The op's `projections()` slot does the lift, preserving sigil
arity:

| Sub-lang (ast-grep) | Host binding |
| ------------------- | ------------ |
| `$NAME`             | `${NAME}`    |
| `$$$ARGS`           | `$$${ARGS}`  |

Author of an `ast[lang]` rule does not retype the arity sigil; the op
reads its `Pattern::meta_vars()` and stamps each capture into the cursor
binding map with the correct arity.
