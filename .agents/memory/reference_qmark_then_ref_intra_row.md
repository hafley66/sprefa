---
name: reference-qmark-then-ref-intra-row
description: "sprf imperative rule-read: ?-bind then same-name ref in same call = intra-row self-equality, not input-cursor correlation"
metadata:
  node_type: memory
  type: reference
  originSessionId: b5f0ade9-540e-4fda-9f7a-284766ab6419
---

MERGED 2026-05-18 main `294d60db` (ff from 8ac78e43, single
commit, gate 496/0/1; worktree removed, branch deleted).

Language rule: in an IMPERATIVE rule-read call (`rule_table_call_pipe`,
`v4/src/sql.rs`), a term BOUND by an earlier arg via `?` (Term-Bind /
`ArgMode::Project`) and then READ by a LATER same-name ref in the
SAME call lowers to an INTRA-ROW self-equality
`__rule.colB = __rule.colA`, NOT a correlated read against the
streaming `input` cursor (`__rule.colB = input.term`). Pre-desugar
that case was a hard SQL error `no such column: input.<term>` (the
term is born in this very read; no prior cursor column to correlate).

So `reaches?(N?, N)` = "the row where this rule's two cols are
equal" (the cycle/self-reach test) — no `where` step needed. The
cycle example `v4/examples/reachability-cycle-imperative.sprf` was
re-pointed off `> where\`${SRC} = ${DST}\`` onto `reaches?(N?, N)`;
render byte-identical (cycle = {a,b,c}).

Mechanics + constraints:
- Forward scan over `ArgMode`s: `Project` records `term -> col`;
  a later `BoundTerm` whose `term` is in the map emits the
  intra-row predicate. Order-honoring: ref-BEFORE-`?`
  (`r?(N, N?)`) keeps the legacy outer-cursor `input.` correlation.
- Map filled ONLY by the Term-Bind subset (`Project`). Literals
  (`:lit`/`"lit"`), `&.value`, and pipe values are INERT — they
  never populate or are rewritten by it. (User's correctness
  catch: "pipes are values, assuming all args are terms is a
  mistake" — scoped to the arg-classification Term subset only.)
- Keys on the RESOLVED TERM name post-kwarg: `r?(X: A?, Y: A)` ⇒
  `__rule.Y = __rule.X` (keyed on `A`, not the column).
- Strictly additive: the rewritten case was previously a hard
  error, so no existing program changes behavior.

RED→GREEN spec: `v4/tests/intra_row_self_eq_target.rs` (positional
`reaches?(N?,N)`, kwarg term-keyed, inert-literal).

Related: [[project-recursion-surface-gaps]] (the cycle example this
desugar simplifies), [[project-dots-types-nesting]] (arg
classification model).
