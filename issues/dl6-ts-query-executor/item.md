---
created: 2026-08-21
updated: 2026-08-21
type: task
status: open
priority: normal
epic: usurp-v4-v5
---

## Description

**Mostly closed.** This card opened as "a `.dl6` program cannot read source
TEXT", covering `match_line`, `match_ast`, `sg`, `ast_yaml`, `comment` and the
`ast` tree-sitter form. `ast_rule` landed on main the same day (`3da1100f2`,
`cd58a6917`) and answers all of them but one.

## What landed

`ast_rule` is a linked in-process executor (`v6/sprefa-engine-rs/src/hosts.rs:48`,
`registry.pl:336` routes on `$SPREFA_AST_RULE_HOST` in the template) answering
`AstRuleMatch` rows whose `captures` carry `name`, `text` and `span`
(`v6/sprefa-extract/src/lang/1_ast_rule.rs:76-91`). The rule algebra is
`Pattern`, `Kind`, `Regex`, `Matches`, `All`, `Any`, `Not`, `Inside`, `Has`,
`Follows`, `Precedes` (each with `stop_by`), plus `fix` producing an
`AstRuleMutationProposal` with a replacement span
(`1_ast_rule.rs:20-101`).

| v5 op | v6 spelling | status |
|---|---|---|
| `match_ast` / `sg` | `rule: {pattern: ...}` | superset: adds `Kind`, `Not`, `Matches`, `utils` |
| `ast_yaml` | `Inside` / `Has` / `Follows` / `Precedes` with `stop_by` | superset: v5 had `inside:` at the immediate parent only |
| `comment` | `all: [kind: line_comment, regex: ...]` plus the two ordering rules | subset: BEGIN/END LIFO nesting is a dl6 join, not an op |
| `match_line` | `rule: {regex: ...}`, usually inside an `all:` with a `kind:` | subset, two named differences below |
| `gen(:replace)` structural rewrite | `fix:` + `AstRuleMutationProposal` + the soopy staging seam | superset |

Exercised end to end by `v6/dl/rails/no-new-eprintln-rail.dl6`
(`just no-new-eprintln`), which ports v5's `match_line` hit rule and both of its
waiver rules onto one host.

## What is still owed

**1. `ast`, the tree-sitter s-expression form.** `ts_query/1` is `live` at
`registry.pl:198` and compiles to a `tree_sitter` host demand
(`plunit_tests.pl:3611-3613`), but `executor_for` has no `tree_sitter` arm
(`hosts.rs:41-59`), so the demand has no linked executor. 11 blocked rails name
`ast`; 4 need nothing else. Fix: one `executor_for` arm plus the query runner.

**2. `match_line`'s two remaining differences**, both named rather than fixed:

- the unit is a GRAMMAR NODE, not a line, so a regex whose match straddles two
  nodes has no spelling. Every rail case measured so far is inside one node.
- a regex NAMED GROUP does not bind a dl var. Captures come from pattern
  metavariables (`$X`), not from `regex:`. v5's `(?<name>..)` has no twin.

Decide whether either is worth closing, or write them down as the shape of the
v6 surface and shut this card.

**3. `scc`.** Strongly-connected-component condensation, 9 blocked rails. It was
filed here because it arrived with the op gaps; it is unrelated to text and
wants its own card if anyone needs it.

## Gate

```bash
cd v6 && timeout 600 just v5-rails      # both ported rails, 12 fixture labels
cd v6/sprefa-extract && timeout 900 cargo test --release --features cli
```
