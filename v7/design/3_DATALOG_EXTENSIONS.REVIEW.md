# DL7 Datalog extensions decomposition review

Status: review against the post-split compiler introduced by `8fda71fa7` and
the four Datalog-extension issue cards landed by `bf2f2bc4c`.

Scope: the relational-cons, stratified-negation, and count-aggregate cards, plus
their claim that userland Pick and Exclude become expressible. This document
separates current and donor facts from proposed contracts.

## Blocking rulings

| Question | Ruling | Blocking consequence |
| --- | --- | --- |
| Relational `cons` | Both an evaluator kernel-clause change and checked authored-order mode validation are required. | An evaluator-only change would make an underconstrained call fail as an ordinary proof failure, so the card's named diagnostic or checked-program refusal criterion could not pass. |
| Checked goal IR | Use `checked_goal(positive, Call)` and `checked_goal(negative, Call)`, where `Call = call(ref(RelationIdentity), Arguments)`. | V6 parser terms such as `not(Goal)` and comma trees must end at lowering. Every checked body item, including a positive one, uses the same carrier. |
| Dense selected-edge indices | A count head can emit the number of strict selected predecessors. The three cards do not provide the strict-predecessor relation and do not cover the zero-predecessor group. | “Pick and Exclude become ordinary prelude rules” and the count card's dense-rank receipt are currently impossible. Add an ordered-index capability and a zero-rank rule or define anchored zero-count semantics. |
| Parallel task execution | None of the three complete implementation cards has a collision-free parallel lane. | Relational cons and negation both change the checker, evaluator, and consolidated test. Negation and count also change the lowerer and share the strata scheduler. |
| Functional dependencies | The live checked relation row has arity only. The settled `':'/4` keys and evaluator key validation are absent. | Pick or Exclude can derive conflicting `(Owner, Name)` or `(Owner, Index)` rows without a checked failure. Restore key metadata and closure validation before accepting either operator. |

The ordering source and empty-list behavior require Chris rulings before an
implementation agent receives the affected card. The bounded implementation
order is in the final section.

The kernel reconciliation already requires safety and functional-dependency
validation, frozen dependency strata, positive tabled closure, completed-lower
negation and aggregates, and cleanup-scoped SWI tables
(`v7/design/0_KERNEL_RECONCILIATION.md:248-306`). The basement plan deliberately
stopped at positive rules and left negation and aggregates outside its three
milestones (`v7/design/2_BASEMENT_TO_DATALOG.PLAN.md:52-72`, `:210-280`). The
Pick and Exclude blocker report accurately records the current missing goal
polarity, reversible list traversal, and dense-rank operation
(`v7/tasks/results/8_PICK_EXCLUDE.md:7-65`).

## Established current facts

### Live phase contracts and row representations

The split compiler has these public predicates:

```prolog
lower_datalog(+Unit, -BasementProgram, -Origins, -Diagnostics).
check_datalog(+BasementProgram, +Origins, -Checked, -Diagnostics).
compile_unit(+Unit, -Compiled, -Diagnostics).
evaluate(+Rules, +Seeds, -Closure, -Diagnostics).
```

The exports and call are at
`v7/src/2_comptime/0_lowerer.pl:1-4`,
`v7/src/2_comptime/1_checker.pl:1-6`,
`v7/src/2_comptime/2_compiler.pl:1-10`, and
`v7/src/2_comptime/2_compiler.pl:45-74`. The compiler passes only `Rules` and
`Seeds` into `evaluate/4`; it does not pass `Relations`, `Depends`, or `Strata`
(`v7/src/2_comptime/2_compiler.pl:66-79`).

The lowerer currently emits:

```prolog
basement_program(
    root_graph(Nodes, PendingEdges),
    datalog_program(Relations, Seeds, Rules)).

relation(RelationIdentity, Arity).
rule(call(name(Owner, Name), Arguments),
     [call(name(Owner, Name), Arguments), ...]).
```

The containing rows are built at `v7/src/2_comptime/0_lowerer.pl:29-40`,
`rule/2` is built at `:226-235`, and each body form goes directly through
`lower_call/5` at `:240-255`. `lower_call/5` accepts one prefix application and
`lower_argument/3` rejects every nested argument form with
`nested_call_argument` (`:257-309`). This rejection applies to a nested
`(count ?Expression)` in a rule head under the current generic path.

The checker currently emits:

```prolog
checked_datalog(
    root_graph(Nodes, ColonEdges),
    datalog_program(
        [relation(ref(RelationIdentity), Arity), ...],
        [call(ref(RelationIdentity), GroundArguments), ...],
        [rule(call(ref(HeadIdentity), HeadArguments),
              [call(ref(BodyIdentity), BodyArguments), ...]), ...]),
    [depends(ref(HeadIdentity), ref(BodyIdentity), positive), ...],
    [stratum(ref(RelationIdentity), 0), ...]).
```

This shape is assembled at `v7/src/2_comptime/1_checker.pl:13-41`.
`resolve_goals/9` still treats each body item as a bare `call/2`
(`:227-243`). `depends_rows/2` hard-codes `positive` (`:302-312`), and
`strata_rows/2` assigns zero to every declared source and kernel relation
(`:314-320`).

The current variable check collects all body variables without considering
authored order or goal polarity (`v7/src/2_comptime/1_checker.pl:286-300`). It
checks only that each head variable occurs somewhere in the positive body. It
cannot validate a `cons` input mode, require a negative goal's variables to be
bound before that goal, or distinguish input and output positions of a
constructive relation.

### Current relation keys

The live declaration is `relation(Relation, Arity)`. The checker sorts complete
rows but carries no `KeySets` field (`v7/src/2_comptime/1_checker.pl:18-21` and
`:322-325`). `dense_index_diagnostics/4` checks authored pending edges only
(`:50-101`). Derived closure rows do not pass through this check.

The settled design gives `':'/4` both `(Owner, Name)` and `(Owner, Index)` as
functional keys and requires zero-based contiguous indices
(`v7/design/1_MINIMAL_VERTICAL_SLICE.PLAN.md:186-197`). It also specifies
`relation(+Name/Arity, +KeySets)` and closure key validation
(`v7/design/1_MINIMAL_VERTICAL_SLICE.PLAN.md:407-421`, `:707-722`). The live
compiler has not carried that part of the design forward.

The kernel inventory remains seven relations:

```prolog
node/1
module/1
product/1
sum/1
':'/4
cons/3
intern/3
```

The declarations are at `v7/src/2_comptime/0_lowerer.pl:323-329`; their graph
rows are at `v7/src/2_comptime/1_checker.pl:147-178`.

### Current evaluator lifetime and cleanup

`evaluate/4` requires ground rule and seed data, allocates one `EvaluationId`,
and wraps installation, collection, and cleanup in `setup_call_cleanup/3`
(`v7/src/1_libtime/0_evaluator.pl:11-24`). Rules and seeds are dynamic facts
keyed by `EvaluationId` (`:6-7`, `:26-39`). Cleanup abolishes that evaluation's
`proves/2` table and erases the exact asserted clause references (`:41-47`).

`instantiate_rule/3` creates one native SWI variable per reified
`var(Identity)` during one proof and discards the map after that proof
(`v7/src/1_libtime/0_evaluator.pl:81-115`). Closure terms outlive the proof;
native proof variables do not.

The existing `cons` clause requires `Head` and `Tail` to be ground and then
constructs `List` (`v7/src/1_libtime/0_evaluator.pl:55-58`). Its two value
clauses encode:

```prolog
cons_value(Head, const(symbol(nil)), const([Head])).
cons_value(Head, const(Tail), const([Head | Tail])) :- is_list(Tail).
```

These clauses are at `v7/src/1_libtime/0_evaluator.pl:69-71`. There is no
ground-list deconstruction clause. A ground `const([])` has no cons tuple in
the current relation.

## Established V6 donor facts

These are donor behaviors rather than recommended V7 term shapes.

1. V6 flattens a Prolog comma tree into authored-order goals and validates the
   sequence left to right (`v6/prolog/0_compiler_relations/0_goals.pl:5-15`).
   Relation goals add their variables to the bound set; scalar binds add only
   their result after checking their inputs; comparisons and negation bind
   nothing and require every variable to be already bound (`:17-50`).

2. V6 classifies `not(Goal)` as a negative relation use and excludes scalar
   binds and comparisons from relation dependencies
   (`v6/prolog/0_compiler_relations/0_goals.pl:72-91`). Those shapes depend on
   the V6 Prolog parser and registry and therefore are not checked V7 IR.

3. The V6 constraint is
   `HeadStratum >= BodyStratum + Gap`. Positive reads have gap 0, negative reads
   have gap 1, and every relation read by an aggregate-headed rule has gap 1
   (`v6/prolog/strat.pl:23-35`, `:44-55`). Relaxation raises strata until stable
   and rejects growth past its derived-relation cap (`:57-78`).

4. The V6 compiler evaluator validates seeds, rules, recursive construction,
   aggregate heads, and functional rows before returning closure
   (`v6/prolog/0_compiler_relations.pl:352-367`). One table namespace closes one
   positive stratum. Negative goals consult only `LowerRows`, which were
   completed before that stratum began (`:433-493`). Cleanup abolishes the table
   and retracts rules, lower rows, and seeds for that `EvalId` (`:438-455`).

5. V6 computes aggregate rows from the rows completed before the aggregate's
   stratum, appends those rows as seeds, and then closes the ordinary rules in
   that stratum (`v6/prolog/0_compiler_relations/1_aggregates.pl:21-40`). A
   strict dependency cycle through negation or aggregation is rejected
   (`:42-119`).

6. A V6 aggregate head classifies each argument as `plain(Expression)` or
   `agg(count, Expression)` (`v6/prolog/0_compiler_relations/1_aggregates.pl:121-136`).
   Plain positions form the group key. Each successful body proof contributes
   one bag entry. A group emits no row when the complete body has no success,
   because `Bag \== []` is required (`:138-184`). Equal contribution values are
   still counted as separate bag entries.

7. V6 functional validation reads declared key positions and rejects two
   unequal rows with the same key values
   (`v6/prolog/0_compiler_relations.pl:520-549`).

## Recommended normalized contracts

### Relation keys and closure validation

Carry functional keys in the existing relation row instead of introducing a
second relation-identity carrier:

```prolog
relation(ref(RelationIdentity), +Arity, +KeySets).

KeySets = [[ZeroBasedPosition, ...], ...].
validate_functional_rows(+Relations, +Rows, -Diagnostics) is det.
```

The required kernel declarations for this slice include:

```prolog
relation(ref(kernel(':')),      4, [[0, 1], [0, 3]]).
relation(ref(kernel(cons)),     3, [[0, 1], [2]]).
relation(ref(kernel(intern)),   3, [[0, 1]]).
```

The `cons` key `[2]` applies to nonempty proper list rows. Relations without a
declared functional dependency use `[]`; exact complete rows still collapse as
sets. Validation runs on the final sorted closure and reports unequal rows that
share any key. This is the V7 `call/2` adaptation of the key-set contract in
`v7/design/1_MINIMAL_VERTICAL_SLICE.PLAN.md:407-421` and the V6 validator at
`v6/prolog/0_compiler_relations.pl:520-549`.

### Positive and negative checked goals

The smallest normalized checked body that preserves the current `call/2` row
and adds polarity is:

```prolog
checked_goal(+Polarity, +Call).

Polarity = positive | negative.
Call = call(ref(RelationIdentity), Arguments).

rule(
    call(ref(HeadIdentity), HeadArguments),
    [ checked_goal(positive,
                   call(ref(PositiveIdentity), PositiveArguments)),
      checked_goal(negative,
                   call(ref(NegativeIdentity), NegativeArguments))
    ]).
```

The source form for a negative goal is the already established prefix form:

```lisp
(not (Relation Argument...))
```

That form appears in the normalized-goal design at
`v7/design/1_MINIMAL_VERTICAL_SLICE.PLAN.md:407-414` and in the prior Exclude
shape at `:579-588`. Lowering consumes the outer `not` form and returns a
polarity value plus one pending call. The checker resolves the pending call and
emits `checked_goal/2`. No `not/1`, comma tree, V6 relation compound, registry
surface term, or parser node survives in `checked_datalog/4`.

Wrapping positive goals as well as negative goals keeps these consumers total:

```prolog
goal_call(+CheckedGoal, -Polarity, -Call).
goal_variables(+CheckedGoal, -VariableIdentities).
goal_dependency(+HeadRef, +CheckedGoal, -depends(HeadRef, BodyRef, Polarity)).
satisfy_goal(+EvaluationId, +CheckedGoal) is nondet.
```

`depends/3` should use the same atoms `positive` and `negative` as
`checked_goal/2`. One vocabulary avoids a `positive|negative` to `pos|neg`
translation inside graph code.

### Authored-order safety and kernel modes

Safety is a left fold over the checked body:

```prolog
check_goal_sequence(+Goals, +Bound0, -Bound, -Diagnostics).
check_goal(+CheckedGoal, +Bound0, -Bound, -Diagnostics).
```

The required transitions are:

| Goal | Preconditions at this authored position | Bound variables afterward |
| --- | --- | --- |
| Ordinary positive relation call | Declared relation and correct arity. | Union with all variables in the call. |
| Negative relation call | Every call variable is already in `Bound0`; target is a row relation admitted for completed-lower-row lookup. | `Bound0`. |
| Positive `cons(Head, Tail, List)` | `List` is bound, or both `Head` and `Tail` are bound. A bound literal or reference counts as ground. | Union with all three argument variables. |
| Positive `intern(Constructor, Arguments, Result)` | `Constructor` and `Arguments` are bound; `Arguments` will be a proper ground list at execution. | Union with `Result` variables. |
| Count head | Every plain head argument and count expression variable is in the final body bound set. | Head checking only; it adds no body binding. |

Every head variable must be in the final bound set. This subsumes the current
unordered head-variable check at `v7/src/2_comptime/1_checker.pl:286-300`.

Negative `cons` and negative `intern` require a separate semantic ruling. They
are evaluator operations and are absent from completed lower-row storage. The
bounded first implementation should reject negative goals whose relation is a
constructive kernel relation.

### Relational cons evaluator contract

The evaluator remains the owner of the two reversible value operations:

```prolog
cons_construct(+Head, +Tail, -List) is semidet.
cons_deconstruct(+List, -Head, -Tail) is semidet.
```

The exact values should remain:

```prolog
cons_deconstruct(const([Head]), Head, const(symbol(nil))).
cons_deconstruct(const([Head | Tail]), Head, const(Tail)) :-
    Tail = [_ | _],
    is_list(Tail).
```

Construction retains the current clauses byte for byte. Dispatch chooses
deconstruction when `List` is ground and construction when `Head` and `Tail`
are ground. With all three arguments ground, either path checks the same tuple.
A ground proper empty list has no head or tail and should fail deterministically
unless Chris selects another empty-list contract. Improper lists fail.

The functional dependencies are:

```text
(Head, Tail) -> List
List -> (Head, Tail), for nonempty proper List
```

The list value and any derived rows live in one closure. Deconstruction creates
no store and no semantic identity. A recursive userland `contains` relation is
bounded by the finite suffixes of its ground proper-list input; the nil tail has
no further cons proof.

### Strata and evaluation order

Use one pure dependency routine for checker receipts and evaluator scheduling:

```prolog
stratify_rules(+Rules, -DerivedStrata, -Diagnostics).
```

For every `depends(Head, Body, Polarity)`:

```text
positive: Stratum(Head) >= Stratum(Body)
negative: Stratum(Head) >= Stratum(Body) + 1
aggregate-headed rule, every relation read:
          Stratum(Head) >= Stratum(Body) + 1
```

All declared refs start at zero. Positive strongly connected components share a
stratum. Any cycle containing a strict edge produces one deterministic
diagnostic before mutable evaluator state is installed. Sorting should use
`(Stratum, RelationRef, authored RuleIndex)` for stable groups while retaining
authored body order inside each rule.

`evaluate/4` can retain its one public entry point because normalized `Rules`
contain polarity and aggregate-head data. The evaluator recomputes the same
derived-head strata with `stratify_rules/3`; the checker unions zero-valued
undeclared-head relations into the `stratum/2` receipt. This avoids changing the
compiler call at `v7/src/2_comptime/2_compiler.pl:74` and avoids passing a second
strata authority into evaluation.

For each stratum in ascending order:

1. `CompletedRows` is the sorted closure of all preceding strata.
2. Aggregate rules for the current stratum fold only `CompletedRows` and emit
   sorted aggregate seeds.
3. Plain rules for the current stratum close positively under tabling. Positive
   goals may read current-stratum tabled rows and completed rows. Negative goals
   read only `CompletedRows`.
4. The sorted stratum closure becomes input to the next stratum.

Validation, stratification, and rule grouping should complete before any
`assertz/2`. Each installed stratum namespace needs
`setup_call_cleanup/3`; cleanup must abolish table subgoals and erase or retract
every rule, seed, and lower-row fact for that namespace on success, diagnostic,
or exception. Aggregate bags and instantiated variable maps are ordinary local
terms and end before the next stratum.

### Count head representation

The source form remains the established prefix nested head form:

```lisp
(count ?Expression)
```

The count-only checked descriptor can be:

```prolog
aggregate(count, Expression).

rule(
    call(ref(Relation),
         [PlainArgument, aggregate(count, Expression)]),
    CheckedGoals).
```

Ordinary head arguments retain their current `var/1`, `const/1`, or `ref/1`
shape. The first implementation should admit exactly one count descriptor in a
head and reject nested forms in every other head position. This is bounded and
does not claim a generic expression path that the live lowerer lacks.

Prefer a deterministic rows signature over a nondeterministic singular output:

```prolog
derive_aggregate_rows(+CompletedRows, +AggregateRule,
                      -SortedRows, -Diagnostics) is det.
```

For one completed input snapshot, the functional dependency is:

```text
(HeadRelation, values of every plain head position) -> count result
```

Count should follow the donor bag law: one contribution per successful complete
body binding, including equal expression values reached through different body
bindings. Exact duplicate complete input rows have already collapsed under set
closure. The acceptance criterion must state this law.

## Dense selected-edge indices

### What count can derive

For a selected input edge at `InputIndex`, a zero-based dense output index is:

```text
count of selected edges whose index is strictly before InputIndex
```

Subtraction is unnecessary when the body already ranges over strict
predecessors. A strict ordering source remains necessary. Current `':'/4` rows
contain integer indices, while the lowerer and checker have no comparison,
subtraction, or predecessor goal (`v7/src/2_comptime/0_lowerer.pl:257-309` and
`v7/src/2_comptime/1_checker.pl:227-284`). The current dense-index predicate
only validates authored edges and does not expose a relation to Datalog
(`v7/src/2_comptime/1_checker.pl:81-101`).

The smallest data-shaped ordering extension is an explicit adjacent row:

```prolog
predecessor(+Owner, +EarlierIndex, +LaterIndex).

% Functional dependency:
(Owner, LaterIndex) -> EarlierIndex
(Owner, EarlierIndex) -> LaterIndex
```

The compiler can derive one row for each adjacent pair of checked colon-edge
indices. Userland derives strict transitive order using ordinary prefix rules:

```lisp
(<- (before ?Owner ?Earlier ?Later)
    (predecessor ?Owner ?Earlier ?Later))

(<- (before ?Owner ?Earlier ?Later)
    (predecessor ?Owner ?Middle ?Later)
    (before ?Owner ?Earlier ?Middle))
```

A checked numeric comparison goal could replace these rows, but that adds a
scalar goal category, ground comparison semantics, and another authored-order
mode. The three current cards include neither contract.

### Exact Pick rank rule shape

Given `contains/2`, `before/3`, and explicitly declared helper relations, Pick's
selected edge and predecessor rows are ordinary prefix rules:

```lisp
(<- (pick-edge ?Input ?Names ?Name ?MemberType ?InputIndex)
    (: ?Input ?Name ?MemberType ?InputIndex)
    (contains ?Names ?Name))

(<- (pick-predecessor ?Input ?Names ?InputIndex ?PriorIndex)
    (pick-edge ?Input ?Names ?Name ?MemberType ?InputIndex)
    (pick-edge ?Input ?Names ?PriorName ?PriorType ?PriorIndex)
    (before ?Input ?PriorIndex ?InputIndex))

(<- (pick-has-predecessor ?Input ?Names ?InputIndex)
    (pick-predecessor ?Input ?Names ?InputIndex ?PriorIndex))

(<- (pick-rank ?Input ?Names ?InputIndex (count ?PriorIndex))
    (pick-predecessor ?Input ?Names ?InputIndex ?PriorIndex))

(<- (pick-rank ?Input ?Names ?InputIndex 0)
    (pick-edge ?Input ?Names ?Name ?MemberType ?InputIndex)
    (not (pick-has-predecessor ?Input ?Names ?InputIndex)))

(<- (: ?Output ?Name ?MemberType ?OutputIndex)
    (Pick ?Input ?Names ?Output)
    (pick-edge ?Input ?Names ?Name ?MemberType ?InputIndex)
    (pick-rank ?Input ?Names ?InputIndex ?OutputIndex))
```

The explicit zero rule is required under the V6 donor behavior because the
aggregate emits no group when `pick-predecessor` has no row. If count is changed
to an anchored correlated aggregate that emits zero for an empty contribution
set, that new semantic contract can replace `pick-has-predecessor/3` and the
zero rule.

Exclude uses the same rank shape after defining its kept edge with a safe
negative membership goal:

```lisp
(<- (exclude-edge ?Input ?Names ?Name ?MemberType ?InputIndex)
    (: ?Input ?Name ?MemberType ?InputIndex)
    (not (contains ?Names ?Name)))
```

All variables read by `not` are bound by the preceding positive colon-edge
goal. Replace `pick-edge`, `pick-predecessor`, `pick-has-predecessor`, and
`pick-rank` with the corresponding exclude helper relations in the five rank
rules above. This duplication is ordinary first-order Datalog; no predicate
parameter or new source form is implied.

The count card alone therefore cannot supply the claimed dense rank. Either of
these additional contracts suffices:

1. explicit `predecessor/3` data plus userland `before/3`, count of strict
   predecessors, and the zero-rank negative rule; or
2. a checked ground integer comparison plus strict-predecessor count and the
   same zero rule; or
3. a nonempty `<=` count plus integer subtraction by one.

Option 1 stays entirely relational after the compiler emits adjacent index
rows. Option 3 requires both comparison and subtraction. The present epic has
none of these options in scope.

## Acceptance-criteria audit

### Relational cons card

Card source: `issues/dl7-relational-cons/item.md:17-47`.

| Criterion | Review |
| --- | --- |
| Existing construction behavior remains byte-equivalent | “Byte-equivalent” has no serializer or byte artifact. Pin exact `==` terms for singleton and longer proper lists. |
| Ground nonempty lists deconstruct deterministically | Implementable with the evaluator clause and exact singleton/tail encoding above. |
| Empty-list behavior is explicit and deterministic | Underspecified. Current facts imply proof failure for `const([])` and use `const(symbol(nil))` as the empty tail sentinel. Chris must confirm this contract. |
| Underconstrained calls produce a named diagnostic or checked refusal | Impossible through an evaluator-only card. Requires checker mode validation, authored-order bound tracking, an exact diagnostic term, and source origin. |
| Reversible cons supports bounded list traversal | Define the admitted domain as finite proper ground lists and state that nil has no decomposition. Improper and cyclic host lists need deterministic refusal or failure. |

### Stratified-negation card

Card source: `issues/dl7-stratified-negation/item.md:17-53`.

| Criterion | Review |
| --- | --- |
| Prefix negative goal lowers | The established spelling is `(not (Relation Argument...))`; the card should pin it explicitly. |
| Checked bodies carry polarity | Pin the complete `checked_goal/2` row, including the unchanged inner `call/2` representation. |
| Negative dependencies impose gap 1 | Implementable. State the inequality and whether `stratum/2` covers all declared relations or derived heads only. |
| Negative cycles produce one deterministic diagnostic | Diagnostic code, relation payload, source origin, and ordering are unspecified. |
| Positive recursive closure remains unchanged | Pin the existing Partial closure snapshot or exact positive-only closure term before restructuring evaluation. |
| Evaluate completed lower strata | `evaluate/4` receives no checked `Strata`. The card must specify shared recomputation from rules or change the input contract. The recommendation above preserves `evaluate/4`. |
| Negative goal safety | Missing. Every negative variable must be bound by preceding goals, and negative constructive-kernel calls need an admit/refuse ruling. |
| Cleanup | Missing from acceptance. New lower-row facts and each stratum's table must be absent after success, diagnostic, and exception. |

### Count-aggregate card

Card source: `issues/dl7-count-aggregate/item.md:17-51`.

| Criterion | Review |
| --- | --- |
| Nested head application lowers through the generic expression path | Impossible in the live lowerer because nested arguments are rejected and no generic expression IR exists. Bound this card to the one `(count Argument)` head case or add a prior expression-IR task. |
| Only count is admitted | Specify whether one or several count positions are admitted. The bounded contract above admits one. |
| Count reads a completed lower stratum | Implementable after the shared strata scheduler. Every relation dependency of an aggregate-headed rule needs gap 1, including positive dependencies. |
| Group keys and output rows are deterministic and sorted | Specify count bag semantics, zero-group semantics, sort order, and conflict behavior across several rules for the same head relation. |
| Aggregate recursion is rejected | Pin separate diagnostics for a strict aggregate cycle and for malformed aggregate placement. |
| Dense-rank receipt | Impossible with the three-card scope. Strict ordering and zero-predecessor handling are absent. |
| Checked aggregate descriptor lifetime | The signature `aggregate_argument(count, +Expression)` does not state the checked rule term. Pin `aggregate(count, Expression)` in the head and erase reader nodes after checking. |

### Epic criteria

Epic source: `issues/dl7-datalog-extensions/item.md:11-35`.

“Pick and Exclude become ordinary prelude rules” is blocked by the missing
ordering source, zero-rank handling, and closure functional-key validation. The
other epic criteria can be made executable after the contracts above are
included. “Compiler and runtime callers retain one evaluator entry point” is
compatible with the recommendation: both continue to call the same
`evaluate/4`, with phase selection and row retention remaining in their callers.

## Collision audit

The compiler split at `8fda71fa7` removes the former monolithic compiler-file
collision. These collisions remain:

| Pair | Real shared writes | Dependency |
| --- | --- | --- |
| relational cons and stratified negation | `v7/src/2_comptime/1_checker.pl`, `v7/src/1_libtime/0_evaluator.pl`, `v7/test/1_entrypoints.test.pl` | Both require the authored-order safety fold and checked-goal dispatch. |
| relational cons and count | `v7/src/2_comptime/1_checker.pl`, `v7/src/1_libtime/0_evaluator.pl`, `v7/test/1_entrypoints.test.pl` | Both extend constructive or aggregate mode checking and evaluator dispatch. |
| stratified negation and count | `v7/src/2_comptime/0_lowerer.pl`, `v7/src/2_comptime/1_checker.pl`, `v7/src/1_libtime/0_evaluator.pl`, `v7/test/1_entrypoints.test.pl` | Count depends on the completed-stratum scheduler introduced for strict dependencies. |

The negation card's `v7-datalog-lower` omission and the count card's inclusion
are accurate relative to cons. Cons adds no source form. The shared test-file
collision is real because the repository has one consolidated compiler oracle
at `v7/test/1_entrypoints.test.pl:43-127`.

No two complete cards should run concurrently. Bounded subwork can proceed in
parallel only after term contracts are frozen and file ownership is disjoint,
for example donor fixture design alongside the `cons` evaluator clause. A
single integration owner still has to land checker safety, evaluator dispatch,
and consolidated snapshots in order.

## Corrected DAG and bounded implementation order

```text
compiler split 8fda71fa7
    |
    v
A. Chris rulings: empty cons; index-order source; zero-count policy
    |
    v
B. checked-program foundation
   B1. checked_goal/2 migration for every positive body goal
   B2. authored-order safety fold and cons/intern mode table
   B3. relation key metadata and closure functional-key validation
   B4. one pure dependency/stratification routine
    |
    +-----------------------+
    |                       |
    v                       v
C. relational cons       D. ordered-index source
   evaluator modes          predecessor rows or checked comparison
    |
    v
E. stratified negation
   source lowering, negative safety, completed-lower scheduler, cleanup
    |
    v
F. count head
   one descriptor, aggregate gap, completed-row fold, bag law
    |
    v
G. userland contains, Pick, Exclude, strict-predecessor ranks, zero ranks
    |
    v
H. one consolidated oracle expansion and exact cleanup receipt
```

`D` is semantically independent of relational list traversal and may be
implemented beside `C` if it owns different files. The three original cards
remain sequential because their current file scopes overlap. `F` follows `E`
in the bounded order so the completed-stratum scheduler has one owner and one
reviewed receipt before aggregate folding is added. Conceptually, negation and
aggregation are sibling consumers of `B4`; sequential landing is a collision
constraint rather than a semantic dependency.

Recommended commit bounds:

1. Foundation only: normalize bodies, safety, key metadata, and pure strata;
   preserve positive closure output exactly.
2. Relational cons only: construction equality, nonempty deconstruction, empty
   behavior, improper-list behavior, underconstrained diagnostic, cleanup.
3. Stratified negation only: one prefix lowering form, safe anti-join, strict
   cycle diagnostic, positive recursion receipt.
4. Count only: one head descriptor, one aggregate per head, completed-row bag,
   strict cycle diagnostic, sorted grouped rows.
5. Ordering only: checked predecessor rows or one checked comparison category,
   with exact ownership and lifetime.
6. Prelude only: `contains`, Pick, Exclude, both rank families, zero-rank arms,
   and functional-key receipts for `':'/4`.

## Choices reserved for Chris

The following choices change source semantics, checked-program semantics, or a
settled public row contract and should not be selected by an implementation
agent:

1. Empty cons: confirm that `const([])` has no `cons/3` tuple and that
   `const(symbol(nil))` remains the sole empty-tail sentinel, or specify another
   exact row.
2. Ordered indices: select explicit `predecessor(Owner, Earlier, Later)` rows,
   a checked ground integer comparison goal, or comparison plus subtraction.
   Also assign ownership of generated predecessor rows if selected.
3. Zero counts: retain the V6 rule that an empty aggregate bag emits no group
   and use the explicit zero-rank negative rule, or define an anchored aggregate
   that emits zero.
4. Functional-key restoration scope: authorize relation key metadata and
   closure validation in the foundation card. Without it, the settled two keys
   of `':'/4` remain unchecked for derived rows.
5. Negative kernel goals: confirm refusal of negative `cons/3` and `intern/3`
   in the bounded evaluator, or define completed-row semantics for them.

The first four rulings block the epic acceptance criterion for userland Pick
and Exclude. Ruling 5 blocks only programs that negate constructive kernel
goals; stratified negation over ordinary declared relations can proceed with a
bounded refusal.
