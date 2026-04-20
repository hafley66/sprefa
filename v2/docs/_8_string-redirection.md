# String redirection / cursor as universal currency [tentative]

Source: chat 2026-04-18. Author flagged the bashism as worth
queueing. None of this is locked; the goal is to preserve the design
direction so it does not get lost.

## The bashism

Bash treats files, pipes, and process I/O uniformly: `>` redirects
stdout to a file, `<` reads stdin from a file, `|` chains processes.
Sprf already has `>` (pipe Seq) and the cursor model that gives
"everything is addressable." A redirection sigil completes the
analogy:

```
> ast[rs](fn $NAME) >>{cursor.slots.signature} render(`${NAME}()`)
```

The `>>{slot_path}` writes the next op's emitted cursor content into
the named slot of the upstream cursor instead of replacing the
content. Reading direction is `&{cursor.content}`,
`&{cursor.fs.parent}`, etc — already the address grammar.

Direction posture, kept consistent:

| Sigil      | Direction | Meaning                                            |
| ---------- | --------- | -------------------------------------------------- |
| `${X}`     | outward   | term decl/ref, fans out into rows                  |
| `&{...}`   | inward    | structural address, reads existing cursor/tree    |
| `>>{...}`  | inward    | redirect: write op output into a cursor slot      |

`>` and `>>` are distinct: `>` is pipe (Seq, downstream takes the
upstream's cursor as input). `>>` is redirect (downstream's output
gets stored into a named slot on the upstream's cursor instead of
flowing forward).

## Cursor as universal currency

Things already addressable via cursor + slot:

- `cursor.content` — bytes
- `cursor.byte_range` — span
- `cursor.fs` / `.repo` / `.rev` — location triple
- `cursor.slots[X]` — op-declared payload
- `cursor.captures[$X]` — term bindings

The `&{...}` address grammar becomes the SQL of the language: one
consistent way to query anything in the cursor world. Worth
promoting `&{...}` from "structural ref" to "the cursor query
language" in the mental model.

## Inline string evaluation

A separate but related idea: `${[expr]}` (or similar) for
"evaluate expr to a string and inline." Distinct from `${X}` which is
a term lookup. Use case:

```
render(`bound to: ${[ &{target.fs.path} | str[basename] ]}`)
```

The pipe inside the brackets evaluates against the current cursor,
the result coerces to string, gets inlined. This is the
expression-language hole that lets render/diag bodies do non-trivial
computation without growing the host grammar.

## Where this matters

- **`lsp[render]` ops** (see `_7_lsp-as-op.md`) need string
  composition with structural data; inline expressions cover that
  without a whole template DSL.
- **`render(...)` op** for documentation generation: read code, emit
  markdown with cursor data interpolated.
- **Test fixture authoring**: redirect test pipeline output into a
  named slot, snapshot from there.
- **Cross-op data flow without breaking the pipe**: when op B
  computes a derived value that op C needs but op B's *cursors*
  should keep flowing forward unmodified, redirect to a slot.

## Why this is queued, not landed

- The host grammar has not absorbed `${X}` / `&{...}` / `$$${X}` yet
  (those are designed but not implemented in tree-sitter).
- The slot system per op is partially specced
  (`project_op_owned_cursor_slots.md` memory) but not built.
- Redirection adds a third sigil family; the term-language doc
  should freeze first so the precedence/lexing story is clean.
- Use cases are real but every one of them has a workaround today
  (just emit two cursor streams and join downstream).

## Backburner items

- `>>{...}` redirect direction (slot write).
- `${[expr]}` inline expression evaluation.
- Promote `&{...}` to "cursor query language" framing in
  `_0_term-language.md`.
- Investigate `<<{...}` for inverse direction (read slot back into
  flow) — likely unnecessary because `&{...}` already reads.

Land these only after layers 1 (parse) and 2 (runtime) are stable in
v3. Premature surface area here ossifies decisions that should fall
out of the runtime shape.
