# Positive dependency cone scheduling

## Change

Each evaluator stratum installs its current nonaggregate rules plus every
nonaggregate definition transitively demanded through a
`dependency(Head, Body, positive, 0, positive)` edge. The dependency rows come
from the existing `rule_dependencies/2` pass. Negative and aggregate edges do
not enter the cone. Aggregate rows are still derived from completed lower rows,
and `install_evaluation/5` receives the same `LowerRows` value.

The dependency graph is now computed once by `evaluate/4`, then shared by
stratification and stratum selection. The public `stratify_rules/3` contract is
unchanged.

For nearest-shadow, installed rule counts at levels 0 through 6 changed from
`[31,42,81,84,87,113,115]` to `[31,25,55,21,19,75,2]`: 553 to 228 rule
assertions per evaluator invocation. Lower-row assertions are unchanged.

## Exact selector coverage

Two tests call the selector with complete rule terms:

1. a positive two-level chain retains both clauses defining its lower relation;
   a negative dependency and an above-level relation are absent;
2. one relation has a plain clause and an aggregate clause. The aggregate
   clause contributes `dependency(shared, aggregate_only, positive, 1,
   aggregate)`, but its lower definition is absent from the selected rules.

Both compare the complete sorted selected rule list, rather than counts.

## Canonical compiler gate

The prechange and candidate captures used the same checkout path:

```text
rows=14586
diagnostics=[]
baseline sha256=c67af22e25b4fb455bd50eab25e9f4e73326be2ee12adfa19d4cbd754475c897
candidate sha256=c67af22e25b4fb455bd50eab25e9f4e73326be2ee12adfa19d4cbd754475c897
cmp=0
```

One candidate run measured 2,153 ms and 14,581,432 inferences. The performance
gate measured cold 2,229 ms and 14,581,425 inferences, then warm 15 ms and 2,144
inferences. Its cold inference delta against the 16,000,000 budget was
`-1,418,575`.

## Output distribution

The 14,586 compiler rows contain 14,586 unique whole rows. Relation counts:

```text
before             13364
kernel(:)             525
kernel(predecessor)   424
kernel(node)          136
kernel(product)       132
kernel(module)          2
closed_names            1
Option                  1
kernel(nil)             1
```

`before/3` owns 91.6% of the closure. Its two triangular expansions are:

```text
module fixture predecessor rows 115 -> before rows 6670 = 115 * 116 / 2
prelude predecessor rows        112 -> before rows 6328 = 112 * 113 / 2
```

The remaining 366 `before/3` rows come from other owners. This fanout is the
next measured target; this change does not alter it.

Repeated identity occurrences are led by the `before` relation identity at
13,376, the fixture module at 6,904, the prelude module at 6,556, `kernel(:)`
at 540, and `kernel(predecessor)` at 434. These are column occurrences across
unique rows, not duplicate facts.

## Validation

```text
selector plus evaluator semantic tests  7/7 passed
lexical binding test 19                 10/10 passed
binding symmetry test 18                16/16 passed in four bounded groups
compiler tracer                          3/3 passed
compiler performance gate               passed
git diff --check                         passed
```

Several existing binding-symmetry compile cases measured from 3.059 to 4.408
seconds under concurrent load. They remain outside this scheduling change.

CI coverage: two evaluator selector cases added; zero changed or removed.
