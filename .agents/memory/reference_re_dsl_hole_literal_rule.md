---
name: re-dsl-hole-literal-rule
description: "sprf re DSL: literal text is ALWAYS raw regex even with ${} holes; literal pipe must be \\| (user ruling: keep regex power)"
metadata: 
  node_type: memory
  type: reference
  originSessionId: b5f0ade9-540e-4fda-9f7a-284766ab6419
---

sprf `re` DSL semantic (RULED + reverted 2026-05-18, main after
`e0a92c20`). `re_body_to_regex` in `v4/src/compile/lower/ops.rs`:
literal text between `${...}` holes passes through UNTOUCHED as raw
regex. The regex crate handles its own escapes:

- `re`${A?}|${B?}`` ⇒ `(?P<A>\w+)|(?P<B>\w+)` = ALTERNATION (B empty;
  A-or-B). NOT a delimiter.
- A literal pipe delimiter MUST be written `re`${A?}\|${B?}``.
- `\d+`, `(?P<NAME>…)`, char classes etc. stay live inside a
  hole-bearing body.

History/why: a 2026-05-18 change made hole-bodies escape literal
metachars (so `|`==`\|`). User RULED against it: "remove the regex
thingy, escaping is fine, we need to be able to regex whenever." Full
regex power beats the `\|` convenience. Reverted; no `dsl_mode` gate.
`${X?}`→`(?P<X>\w+)` (tight), `$$${X?}`→`(?P<X>.*?)` (loose), `${X}`
Read rejected — those carveouts kept; only the literal-escaping was
removed. Pinned: `v4/tests/dsl_hole_grammar_target.rs::
re_hole_body_keeps_raw_regex`. Examples use the escaped `\|` form
(`repo-rev-discovery-graphviz.sprf`, `reactive-reach-retraction.sprf`).
`split_line()` (paren, no arg) works = `split`\n`` for multiline
literal blocks. Related: [[project-recursion-surface-gaps]].
