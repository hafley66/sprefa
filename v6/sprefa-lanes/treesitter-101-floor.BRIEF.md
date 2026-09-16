# LANE: tree-sitter 101-char floor, user delegated the call

## FIRST ACTION, NON-NEGOTIABLE
```
git merge --ff-only 9ecd3341
```
Failure or missing trees = STOP AND REPORT. No archive/tar/copy workaround.

## WORKTREE SETUP BEFORE FIRST COMMIT (never `--no-verify`)
1. copy `v6/sprefa-extract/target/release/extract` from the main tree in
2. `cd v6/tsv2 && pnpm install`
3. `cd v6/sprefa-store/js && pnpm install`

## CONTEXT
Read `plans/2026-08-11-tree-sitter-door.PLAN.md` sections at :237-279 first.
Current overlay ratio 0.1021, overlay floor 445 non-ws chars, table at :269.

## THE USER'S WORD, VERBATIM
"i got zero opinions about auto tree sitter outputs from parser"

=> The two rows marked "User call" are now YOURS to decide. Decide them,
implement, and WRITE DOWN the reasoning you used. Do not come back asking.

| rule | chars | the question |
|---|---:|---|
| `enum_variant` | 72 | editor is wider than the parser: `enum_field//1` types a field with `ident//1`, the editor rule uses `$.type` |
| `query` | 29 | editor is wider than the parser: editor keeps the `$.atom` node; `query_stmt//1` inlines `ident//1`/`head_args//1` and refuses dotted paths |

Guidance, not an order: an EDITOR grammar being wider than the parser is
normally fine and often better, because an editor highlights half-typed text
the parser would reject. If you take that reading, say so and make it uniform
across both rows rather than deciding each ad hoc.

## THE OTHER 344 IS NOT YOUR IMPLEMENTATION JOB
`expression` (206), `unary_expression` (71) and `column` (67) are blocked
structurally, per the same table:
- expression needs editor precedence tiering, not a fact row
- unary_expression has NO DCG clause at all; the parser reads a leading minus
  inside `int_lit//1`/`float_lit//1`, so there is nothing to hang a node on
- column: `typed_col//2` defers its type parser through `call(TypeP, Col, Type)`
  with `TypeP` unbound at parse time; its two concrete bindings differ only in
  a cut, and merging passes parity while SILENTLY WIDENING THE LANGUAGE. A
  four-agent precedent left it unmerged. Do not merge it.

Your job on these three is a written ASSESSMENT ONLY: what would each cost,
and is it worth it. No code. Silently widening the language is a defect.

## FILES YOU OWN
```
v6/dl/grammar/           (tree-sitter grammar + overlay)
plans/2026-08-11-tree-sitter-door.PLAN.md
```
Concurrent lanes own `v6/prolog/0_type_plane.pl`, `v6/prolog/lower.pl`,
`v6/prolog/compile/registry.pl`, `v6/prolog/conformance/body.pl`,
`v6/prolog/compile/scripts/0_json_arrival.pl`,
`v6/prolog/compile/6_emit_dd_plan.pl`, `v6/prolog/compile/test/`.
DO NOT EDIT ANY OF THOSE. If you need one, STOP AND REPORT.

## GATE (run, paste output)
```
python3 measure.py            # the ratio measurement the plan doc uses
cd v6/prolog && swipl -g go -t halt ARCH.pl
just green-all
```
Report the ratio BEFORE and AFTER. Baseline 0.1021, overlay 445.
Battery baseline: conformance 281/0, plunit 276, TEXT_DOOR 196/196/0,
tsv2 128/1skip, store 74/74, dl 96/96.

## A WARNING FROM THE LAST ROUND
Round 4 Part A made the ratio WORSE (0.3180 -> 0.3427) and that was CORRECT:
field names land in grammar.js before the emitter can generate those rules, so
they sit in the overlay until Part B. A lane that banked A and stopped would
have reported a regression. If your intermediate step regresses the ratio, say
why and keep going; do not stop at a local worsening.

## ANTI-CHEAT
| banned | why |
|---|---|
| `--no-verify` | the rail is the gate |
| merging `column`'s two bindings | passes parity, silently widens the language |
| widening a fixture to match output | that is deleting the test |
| claiming a ratio you did not run | every number is pasted tool output |
| editing files outside your list | disjoint ownership |
| asking the user to decide the 101 | the user delegated it to you, in writing, above |

## STYLE LAWS
No em dashes. Banned in prose AND identifiers: `provenance`, `substrate`,
`load-bearing`, `regime`. "refusal" banned in prose, say TODO or "not built
yet". Comments state ONLY constraints the code cannot show, no change-log
narrative, no dates, no arc references. Descriptive variable names, never
single-letter. Construct names use ONLY rxjs, prolog, or SQL vocabulary.
Colocated consistency inside a file.

## COMMIT OFTEN. A prior lane lost a whole run to a machine sleep.

## REPORT
`REPORT.md` at the worktree root: (1) the two decisions and your reasoning,
(2) before/after ratio with pasted `measure.py` output, (3) the written
assessment of the 344 with a cost per rule, (4) every gate command with pasted
output, (5) what you did NOT do and why. Do not open a PR. Do not spawn
subagents; lanes never fan out.
