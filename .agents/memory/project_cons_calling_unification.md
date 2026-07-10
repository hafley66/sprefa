---
name: project-cons-calling-unification
description: "cons/calling unification plan: BUILDABLE, ROUTE A + {-as-op + & cursor sigil, 8-step order"
metadata: 
  node_type: memory
  type: project
  originSessionId: b5f0ade9-540e-4fda-9f7a-284766ab6419
---

`plans/2026-05-19-cons-calling-unification.md` promotes the frozen
`chat_log/20260516.1` cons design (list+kwarg = one cons cell; root =
implicit body) to an actionable plan. 3-agent feedback (grammar/runtime/
type-model) converged on 3 FATALs; user locked the forks 2026-05-19:

- **D-R3**: root spelled/modeled `{}` + fanout the seed, but TOTAL
  SOURCE ORDER kept for exec + rule binding. `{}` ≠ unordered. Empty
  root = unit `1`. Pure lower-time relabel, grammar FROZEN, `;` stays
  a mandatory terminator ⇒ zero-regression by construction.
- **D-TY**: cons cell `ty: Option<Value>` (value-space types, honors
  the 2026-05-19 [[project_types_in_value_space]] ruling), NOT `Arc<str>`.
- **D-LIT**: standalone cons-list value IS in scope, satisfied by
  **ROUTE A** (cons is an OP; `()`/`{}` literal sugar desugars to
  `cons`/`merge` op-invocations in the walker, reusing existing
  `op_invocation`+glued `paren_slot` ⇒ zero new production, zero LR
  conflict, zero regen). ROUTE B (bare-paren + `conflicts:`/scanner)
  kept only as escape. ROUTE A under independent grammar validation.
- **D-Q1**: Python rule (positionals before keyed), keep existing
  `lower/positional-after-kwarg` error verbatim.
- **STEP 0**: `?`-surface fix FIRST. `x?: t.i64` does NOT lex as a
  kwarg today (split_keyword_arg key=`"x?"`, is_ident false ⇒ whole
  cell = 1 positional slot, `?` swallowed, walk.rs:557). The
  callable-value plan H8 + an earlier in-chat explanation both wrongly
  said "lexes as a kwarg"; corrected in both plan files. Decl-mark
  CONS-OF-CONS (user, 2026-05-19): `Cons` is PURE `{key,value}`, no
decl/ty fields (betrays "cons is bottom"). `ValueKind` gains
`ConsList` so a cell value can be a cons-list. Decl col = a cell
whose value is a sub-cons-list with reserved cells `decl`/`ty`,
addressed by the SAME `resolve_dot` walk = the existing `DotTable`
(`.map`=named cons-list, `.ty`=the ty cell; value.rs:26, ctx.rs:272).
Typed cols FOLD INTO dots. `decl` at classify, `ty` value-space at
lower.

`{`-IS-AN-OP (user, 2026-05-19, "bash [[ rule"): bare `{...}` at
pipe-step lowers as the merge op, the `{` token itself names it (no
`merge` keyword). One grammar.js diff: add `brace_block` to
`_pipe_step` alts; no `conflicts:` (validated brace-collision-free).
Walker synthesizes merge-op invocation from already-classified cells.
The `(` analog is DEFERRED (parenthesized conflict, real grammar
work); bare seq stays `cons(...)` until that lands.

`&`-IS-CURSOR (user, 2026-05-19): reserved name `&` = a Value view
of the upstream cursor; pre-bound by the walker at each step
boundary. `&.value`, `&.at`, `&.terms.X` resolve through the EXISTING
`resolve_dot` (ctx.rs:272) — cursor wrapped as a Value whose
DotTable.map exposes its fields. DSL `${&.value}` reuses the existing
dsl-interp branch. Future RED:
`` `lol` > { x: str`${&.value}.lol2` } `` ⇒ `{x: "lol.lol2"}`.
- **D-Q3owner**: keyed vs positional spellings must canonicalize to
  one positional order before RuleInvokeAssign/cache_key (rule.rs:172-237)
  or double-MATERIALIZE.

`CallArg{keyword,value}` (value.rs:50) is the proto-cons; →`Cons`
rename mechanical (35 refs/7 files); normalize_call_args + the 158
ArgKind/validate refs are semantic. This is the *args* side of
[[project_cross_file_entity_graph]]-adjacent `apply`; land
callable-value (feat/callable-value c79c47f8, GREEN unmerged) first,
then generalize its `Vec<Value>` arg to `ConsList`. Recursion fixpoint
itself is order-independent; only cross-statement sequencing + lower-
time rule visibility break under unordered root (hence D-R3).
