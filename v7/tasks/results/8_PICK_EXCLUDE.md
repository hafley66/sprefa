# DL7 Pick and Exclude expressibility blocker

Date: 2026-08-29

Status: blocked; `issues/dl7-pick-exclude` remains open.

## Checked representation

`checked_datalog/4` stores `datalog_program(Relations, Seeds, Rules)` where
each rule body is a list of `call(Relation, Arguments)` terms
(`v7/src/2_comptime/0_compiler.pl:353-355`).  There is no goal-polarity
representation.  `depends_rows/2` emits only
`depends(HeadRef, BodyRef, positive)` (`:625-634`), and `strata_rows/2` emits
`stratum(Relation, 0)` for every declared relation (`:636-642`).

Therefore a completed lower-stratum anti-join for `Exclude` has no checked
term to lower to, no negative dependency row, and no nonzero stratum.

## Evaluator

`proves_body/2` proves every body item through `proves/2`
(`v7/src/1_libtime/0_evaluator.pl:64-67`).  The only constructive kernel
clauses are:

```prolog
call(ref(kernel(cons)), [Head, Tail, List])
call(ref(kernel(intern)), [Constructor, Arguments, Result])
```

The `cons/3` clause requires both `Head` and `Tail` to be ground before it
constructs `List` (`v7/src/1_libtime/0_evaluator.pl:55-58`); its values are
only `const([Head])` and `const([Head | Tail])` (`:69-71`).  It cannot
deconstruct a supplied list into head and tail.  The complete checked kernel
relation inventory is `node/1`, `module/1`, `product/1`, `sum/1`, `:/4`,
`cons/3`, and `intern/3` (`v7/src/2_comptime/0_compiler.pl:461-467`).
Consequently a userland positive symbol-membership join for `Pick` cannot
traverse the supplied symbol list.

## Dense output indices

The only dense-index check is `dense_index_diagnostics/4`, which validates
source bind indices (`v7/src/2_comptime/0_compiler.pl:364-407`).  The checked
relations have no arithmetic, comparison, aggregate, or rank representation.
Reassigning indices after selection requires the count of preceding selected
edges, and no existing relation can derive that count.

## Lowering

`lower_goals/7` lowers every rule-body form through `lower_call/5` to a
positive `call/2` (`v7/src/2_comptime/0_compiler.pl:237-285`).  The parser
and lowerer contain no negative-goal syntax or lowering case.  Adding an
anti-join would require a new syntax and checked representation in addition
to stratified evaluation.

## Missing predicates and representations

| Requirement | Missing predicate or representation |
| --- | --- |
| Pick membership | list destructuring or `member/2`-equivalent relation |
| Exclude anti-join | negative goal representation, negative `depends/3` row, nonzero strata, completed-stratum evaluator step |
| Dense output indices | predecessor-count or dense-rank aggregate/relation |
| Negative source rule | negative-goal syntax lowering to checked Datalog |

No prelude, fixture, oracle, compiler, or evaluator change was made.  The
focused SWI command was not run because the stop condition was reached before
implementation.
