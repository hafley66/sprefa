---
created: 2026-08-21
updated: 2026-08-21
type: bug
status: fixed
priority: normal
epic: usurp-v4-v5
closed: 2026-08-21
---

## Description

**CLOSED by the door landing.** This card said the `no-new-eprintln` rail could
not be ported because no v6 record carried "this line calls `eprintln!`". That
was measured and true on 2026-08-21 morning. `ast_rule` landed on main the same
day (`3da1100f2` `v6/extract: wire typed ast-grep rules through DL6 hosts`,
`cd58a6917` `v6: adapt ast-rule host to row executor seam`) and answers both
halves of the rail.

The port is `v6/dl/rails/no-new-eprintln-rail.dl6`, gated by
`just no-new-eprintln`.

## The mapping

| v5 line | v5 construct | v6 spelling |
|---|---|---|
| `.dl/no-new-eprintln.dl:25` | `match_line(f, rev, /eprintln!/, line)` | `all: [kind: expression_statement, has: {pattern: eprintln!($$$ARGS)}]` |
| `.dl/no-new-eprintln.dl:32` | `comment(f, rev, /@eprintln-ok:/, line)` | `follows: {all: [kind: line_comment, regex: '@eprintln-ok']}` |
| `.dl/no-new-eprintln.dl:42` | `match_line(f, rev, /\/\/.*@eprintln-ok:/, line)` | `precedes: {...}`, the same rule the other way |

v5 needed two waiver rules because its `comment` op saw only whole-line
comments. One `any: [follows, precedes]` covers both here, with no line
arithmetic: `follows` is v5's `waiver_line == line - 1` and `precedes` is its
trailing case.

## The v6 rule is strictly more faithful than v5's

A MULTI-LINE `eprintln!(` whose marker sits on the closing `);` line. v5's
window is `[line-1, line]` against the `eprintln!` TOKEN line, so it never sees
a marker four lines down and reports a waived print as a finding. The
statement's next sibling is that comment either way.

Two of v6's own 17 sites are this shape
(`v6/sprefa-engine-rs/src/bin/emit_rust_harness.rs:89` and `:306`). A plain
grep with v5's rule reports FIVE survivors; the rail reports three, and the
rail is right. `fixtures/eprintln/multiline_waiver.rs` pins it.

## Receipts

```
just no-new-eprintln
hits=7 waived=3 new=4 exceeded=0
ok     bare.rs                  no baseline row and no waiver, one row per site
ok     waived_above.rs          the comment-above form, v5's waiver_line == line - 1
ok     waived_trailing.rs       the trailing form v5 needed a second rule to see
ok     near_miss.rs             a marker neighbouring another statement waives nothing
ok     clean.rs                 tracing only, no print to find
ok     multiline_waiver.rs      a marker on a multi-line call's closing line, which v5 missed
NO-NEW-EPRINTLN OK  findings=4
```

Over the shipped crates: `hits=17 waived=14 new=0 exceeded=0`.

## Named limit, carried into the program header

The unit is `expression_statement`, so an `eprintln!` in a non-statement
position (a match arm expression, a tail expression) is not seen; v5's line
regex saw those. Zero such sites exist in v6 today. Widen the `kind` list when
one appears rather than going back to a line scan.
