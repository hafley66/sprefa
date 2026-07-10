---
name: project-callable-value
description: "callable-as-a-Value — MERGED to main as e3ea0aa on/before 2026-05-19; precondition for typed columns + type-IR plan now cleared"
metadata: 
  node_type: memory
  type: project
  originSessionId: b5f0ade9-540e-4fda-9f7a-284766ab6419
---

`plans/2026-05-18-callable-value.md` (PATCHED 2026-05-19 with
code-grounded `## Corrected` H1-H7 after a hole-poke): `ValueKind`
widened with `Callable(CallableRef{kind:CallableKind,name})`;
free fn `apply(ctx,&Registry,&CallableRef,Vec<Value>) -> Value`
(Op arm = real 7-arg `Registry::lower`, `Vec<Diag>`→`LowerError::Validate`;
Rule arm = existing `RuleInvokeComponent`); `ArgKind::Callable`;
`resolve_dot` Callable(Rule) arm; `RuleInvokeComponent::describe()→Some(self)`
(one justified deviation, for H7 structural pin).

MERGED to main as commit `e3ea0aa feat(lower): callable as a value
(Value::Callable + apply)` on/before 2026-05-19. Confirmed 2026-05-20
via `git log main..feat/callable-value = 0`. The local branch
`feat/callable-value` and its worktree at
`/Users/chrishafley/projects/sprefa-callable` are leftover housekeeping
and can be reclaimed (~5.2G own target dir).

Previously the memory said "GREEN, unmerged" and several plans
(`plans/2026-05-20-refactor-audit-plan.md` Phases 6 + 7,
`plans/2026-05-20-cross-language-module-graph-plan.md` cross-refs,
[[project_type_ir_value_space_plan]] precondition) deferred work behind
this merge. All such gates are now cleared.

Internal plumbing, NO sprf surface yet. It is the PRECONDITION for
typed columns / `t.i64`-as-pipe, NOT those themselves. H8 CORRECTED:
`x?: t.i64` does NOT lex as a kwarg (earlier claim wrong) — it lexes
as NEITHER; `split_keyword_arg` key=`"x?"`, is_ident false ⇒ one
positional slot, `?` swallowed (walk.rs:557). The args side of the
same `apply` is generalized by [[project-cons-calling-unification]]
(`Vec<Value>`→`ConsList`). Types must be value-space per
[[project_types_in_value_space]].
