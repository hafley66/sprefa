# merge_family: does the rxjs merge family lower to plain rules?

Lab: `v6/prolog/labs/merge_family.pl`.
Run: `swipl -q -l v6/prolog/labs/merge_family.pl -g go -g halt` (12 PASS, exit 0).
Receipts: `swipl -q -l v6/prolog/labs/merge_family.pl -g report -g halt`.

The lab carries a reference interpreter for one tick:
`tick(Program, StartRows, Arrivals, NextRows, Deltas)`. Level rules (`<-`) are a
membership view recomputed from the current rows, so consequences retract. Edge
rules (`<+`) fire once per arriving body atom joined against the current sets and
append. A rel declared `keyed(Rel/Arity, KeyPositions)` holds at most one row per
key; a new derivation replaces and the tick emits `-old` then `+new`.

## Verdict per operator

| operator | verdict | why |
|---|---|---|
| `merge` | lowers clean | two edge rules, one head. No new construct, no key, no `pre`. Per-tick batching falls out of the tick being the batch. |
| `mergeByKey` | lowers clean across ticks, needs a semantics ruling within a tick | last-write-per-key is the `Key(Type)` column and nothing else. Two writers in the SAME tick have no order to disagree about, so the conflict law rejects the program. rxjs serializes instead. That divergence is a decision, not a bug, and it is currently unwritten. |
| `mergeByKeyScan` | needs a semantics change | rules express the fold, and the fold is wrong the moment a tick carries more than one event for a key. Two increments in one tick produce a count of 1 and raise no conflict, because both arms derive the identical row. |

## Receipts

Delta ticks are printed as emitted: retractions first, then assertions, each in
standard term order. Source-rel deltas are shown because they are observable
rows too.

### merge, two sources into one head

```
out(Item) <+ event_a(Item);
out(Item) <+ event_b(Item);
```

```
tick 1  [+event_a(alpha),+out(alpha)]
tick 2  [+event_a(gamma),+event_b(beta),+out(beta),+out(gamma)]
tick 3  [+event_b(delta),+out(delta)]
final   [event_a(alpha),event_a(gamma),event_b(beta),event_b(delta),
         out(alpha),out(beta),out(delta),out(gamma)]
```

Tick 2 carried an arrival from each source and emitted both derived rows in that
one tick. No `-out(...)` appears anywhere in the trace, which is the edge law
holding. Graded by `merge_batches_per_tick` and `merge_never_retracts`.

### mergeByKey, two writers across ticks

```
rel latest(key: Key(Str), value: Str);
latest(Key, Value) <+ from_poll(Key, Value);
latest(Key, Value) <+ from_push(Key, Value);
```

```
tick 1  [+from_poll(cli,v1),+latest(cli,v1)]
tick 2  [-latest(cli,v1),+from_push(cli,v2),+latest(cli,v2)]
tick 3  [-latest(cli,v2),+from_poll(cli,v3),+latest(cli,v3)]
final   [from_poll(cli,v1),from_poll(cli,v3),from_push(cli,v2),latest(cli,v3)]
```

The replacement sequence is exactly `-old` then `+new`, once per tick, and the
final row is the last writer. Graded by `key_last_write_wins`.

Equal-row case, the second writer deriving the row that is already there:

```
tick 1  [+from_poll(cli,v1),+latest(cli,v1)]
tick 2  [+from_push(cli,v1)]
final   [from_poll(cli,v1),from_push(cli,v1),latest(cli,v1)]
```

Tick 2 is silent on `latest`. That is this interpreter's choice, and it is
ambiguity 1 below. Graded by `key_identical_write_is_silent`.

Same-tick case:

```
key_same_tick_conflict
  REJECTED keyed_conflict(latest/2,[cli],[latest(cli,v1),latest(cli,v2)])
```

Graded by `key_conflict_rejected`.

### mergeByKeyScan, a counter folded from two event rels

```
rel counter(name: Key(Str), total: Int);
counter(Name, Next) <+ increment(Name, _), pre(counter(Name, Total)), Next is Total + 1;
counter(Name, Next) <+ decrement(Name, _), pre(counter(Name, Total)), Next is Total - 1;
hot(Name) <- counter(Name, Total), Total >= 2;
```

Hand-computed state from a seed of 0: 1, 2, 1, 2. Measured:

```
tick 1  [-counter(clicks,0),+counter(clicks,1),+increment(clicks,ev1)]
tick 2  [-counter(clicks,1),+hot(clicks),+counter(clicks,2),+increment(clicks,ev2)]
tick 3  [-hot(clicks),-counter(clicks,2),+counter(clicks,1),+decrement(clicks,ev3)]
tick 4  [-counter(clicks,1),+hot(clicks),+counter(clicks,2),+increment(clicks,ev4)]
final   [hot(clicks),counter(clicks,2), ...events...]
```

Graded by `counter_fold_matches_hand_computation`. The `hot` line is the level
rule earning its arrow: it appears at tick 2, retracts at tick 3 when the count
falls under the threshold, and returns at tick 4, all without a rule saying so
(`level_view_retracts_over_keyed_state`).

Seed arm and transition arm under one key, made disjoint by negation:

```
counter(Name, 1)    <+ increment(Name, _), not(pre(counter(Name, _)));
counter(Name, Next) <+ increment(Name, _), pre(counter(Name, Total)), Next is Total + 1;

tick 1  [+counter(clicks,1),+increment(clicks,ev1)]
tick 2  [-counter(clicks,1),+counter(clicks,2),+increment(clicks,ev2)]
```

No conflict is raised, because exactly one arm can fire per key per tick. Graded
by `seed_and_transition_are_jointly_semidet`. This is LANG.md's "yield points
separate seeds from transitions" working, and it is also the reason the conflict
check cannot be a syntactic one (see tier order below).

Conflict, one increment and one decrement in one tick:

```
counter_same_tick_conflict
  REJECTED keyed_conflict(counter/2,[clicks],[counter(clicks,-1),counter(clicks,1)])
```

Graded by `scan_conflict_rejected`.

### The hole the conflict law does not cover

Two increments for the same key in one tick:

```
tick 1  [-counter(clicks,0),+counter(clicks,1),+increment(clicks,ev1),+increment(clicks,ev2)]
final   [counter(clicks,1), increment(clicks,ev1), increment(clicks,ev2)]
```

Two events arrived, the counter moved by one, and nothing was rejected. Both
derivations read the same `pre` and produced the identical row `counter(clicks,1)`,
so the per-key check saw one distinct row and passed. Graded, as the defect it
is, by `scan_undercounts_batched_events`.

This is the whole gap between "several rules writing one keyed rel" and a scan.
A scan is a fold over an ordered sequence. A tick is a set with no order and no
multiplicity. Rules can express last-write-wins because that needs no order past
the tick boundary. They cannot express accumulation, because accumulation needs
to see N events as N steps.

## Numbered ambiguities found in LANG.md

1. **A keyed edge rule whose derived row EQUALS the existing row: no-op or
   `-x`/`+x`?** LANG.md says "new derivation REPLACES (emits -old/+new)" without
   saying whether an equal derivation is a derivation. This lab chose no-op, so
   the tick is silent. Both readings have a caller. No-op keeps downstream edge
   rules from re-firing on an unchanged value, which is what a merge of two
   pollers wants. Emitting `-x`/`+x` is what SWR revalidation wants, because a
   200 that returns the same body still refreshes the written-at field and still
   is an event. Under count-IVM the two readings differ again: a second origin
   adds support to the row, which is neither a no-op nor a replacement.
   Recommended split: the answer is no-op for row equality, and the SWR case is
   served by putting `written_at` in the row, which makes the rows unequal and
   the question disappear.

2. **"Jointly semidet per key per tick" is ambiguous between two readings.**
   Mercury semidet means at most one SOLUTION. This lab implemented at most one
   distinct ROW, which is strictly weaker and is why case 6 above passes. The
   strict reading would reject the double increment, correctly labelling it as a
   program the tier cannot run. The weak reading accepts it and computes the
   wrong number. Pick one in writing.

3. **Event rels are sets, so identical events in one tick collapse.** This lab
   had to add an id column to `increment/decrement` for two clicks to be two
   rows at all. LANG.md's arrival-tick salt note is filed under demand rows only;
   the same disease hits every event rel feeding a scan. Either every event rel
   carries an identity column by construction, or the surface has no way to
   observe two of the same thing.

4. **Intra-tick order between edge writes and the level closure is
   unspecified.** "A body is one time cut" does not say which cut. This lab runs
   level closure, then edge firing against that closure, then edge writes, then
   level closure again. Consequence: an edge rule cannot be triggered by a level
   row that exists only because of this tick's edge writes. A joint fixpoint
   would allow it and would need a termination argument, since keyed replacement
   can oscillate. Two phases is the conservative choice and it should be stated.

5. **Every arm of a scan reads the same `pre`.** LANG.md's `pre` is T-1 and
   ARCH.pl's "THE REGISTER ROW IS pre" agree with that, and the consequence is
   not written down anywhere: arms of a keyed scan do not compose within a tick.
   Redux's combineReducers composes because each action is its own step. Here
   all arms are one step.

6. **"Consequences never retract" and keyed replacement contradict each other at
   the surface.** A keyed EDGE head emits `-old`. The trace shows `-hot(clicks)`
   at tick 3, a retraction that flowed out of an edge-written rel into a level
   view. The edge law holds for unkeyed heads only. The two bullets sit adjacent
   in LANG.md without reconciling; the rule is that the key, not the arrow, owns
   retraction.

7. **Nothing bounds the source rels.** `final` for the merge trace holds every
   event ever delivered, and `out` is then a copy of their union. ARCH.pl has
   `retention_bound` as an algorithm; LANG.md's surface has no way to say it.
   Any rel that is only ever appended to grows without limit, and the late
   subscriber replay note is the same fact seen from the other side.

## The syntax experiment

The ask was whether a multi-source scan can read as one declaration-adjacent
cluster. The lab implements a `scan` cluster as `term_expansion`, no new arrow:

```prolog
scan(counter(Name), pre(Total),
     [ arm(increment(Name, _), Total + 1)
     , arm(decrement(Name, _), Total - 1)
     ]).
```

It expands into program 3's two edge rules and is graded two ways:
`scan_sugar_expands_to_plain_rules` (the expansion is a variant of the
hand-written rules) and `scan_sugar_trace_identical` (byte-identical delta
trace on the counter scenario).

**Verdict: it does not earn its keep. Keep plain rules.** Two reasons.

- The saving is small and it is paid for in a new bracket shape. Four source
  lines replace two, and the two things stated once (the head rel and the
  `pre` read) are exactly the two things a reader wants to see on the line that
  writes them.
- It reads as if the arms compose, redux-style, one action at a time. They do
  not (ambiguity 5). Sugar that makes a wrong mental model easier is worse than
  the verbose form. The double-increment receipt above is what a reader of the
  cluster form would never predict.

The cheaper win for "flowing" is not a new construct. Allow an expression in a
head column and every arm is one line, with the arms contiguous under the rel
declaration:

```
rel counter(name: Key(Str), total: Int);

counter(Name, Total + 1) <+ increment(Name, Event), pre(counter(Name, Total));
counter(Name, Total - 1) <+ decrement(Name, Event), pre(counter(Name, Total));
```

That is already a declaration-adjacent cluster, it introduces nothing, and it
removes the `Next is ...` goal that carries all of the noise in the prolog
encoding. Head-position expressions are a separate question (they collide with
term-valued columns: is `a + b` an expression or a stored term) and want their
own lab.

Where `pre` is genuinely needed, from the traces: only when the new value is a
function of the old one, which is exactly mergeByKeyScan. mergeByKey needs no
`pre` at all, and the lab's `merge_by_key` program has none. If an arm's value
depends only on the event, the rel is a mergeByKey and the key alone suffices.

## Deviations from the LANG.md snapshot

- Rules are prolog terms under `op(1150, xfx, <-)` and `op(1150, xfx, <+)`, read
  by a `term_expansion` hook so the lab body reads close to the surface. Column
  types and the `rel`/`enum` declarations are not modelled; keys are declared as
  `decl_of(Program, keyed(Rel/Arity, KeyPositions))`.
- Event rels carry an id column that LANG.md's examples do not show. Without it
  ambiguity 3 makes the scan traces untestable.
- One rel gets one rule kind in this lab. ARCH.pl's mixed-head callout says
  count-IVM makes mixed heads sound; nothing here tests that.
- Negation is written `not(Goal)` and is stratified by construction (it only
  ever wraps `pre`).

## What this means for the tier order

1. **`mode_lab` grows a job and moves earlier.** The conflict law is a runtime
   throw in this lab. LANG.md wants the checker to discharge it. Program 4 shows
   it cannot be syntactic: the seed arm and the transition arm head the same
   keyed rel and are safe only because `not(pre(...))` makes their bodies
   disjoint. So the check is pairwise body disjointness over the set of rules
   heading each keyed rel, which is a satisfiability question over bodies, not
   the per-rule fold that `plans/2026-07-27-mode-dominance.md` scopes. Budget
   for that before `register_lowering`, because UPDATE..CASE per keyed rel is
   only sound once the arms are proven disjoint.

2. **`register_lowering` is blocked on the multiplicity ruling.** A per-key
   UPDATE..CASE can only write one value per key per tick, which is the exact
   shape that undercounts. Either the surface gains an ordered per-key fold over
   the tick's arrivals (an aggregate head, not a rule), or scans are declared to
   see one event per key per tick and the checker rejects the rest. Emitting SQL
   before that ruling bakes the undercount in.

3. **The sugar table does not grow.** merge and mergeByKey need no `sugar/2`
   entry in `src/kernel.pl`; they are `rule` plus `keyed_rel`. mergeByKeyScan
   needs `rule`, `keyed_rel`, and `pre`, and the rejected `scan` cluster would
   have been a fourth entry buying nothing. This supports the boil-pot kernel
   candidate `{ground_terms, rule(level|edge), keyed_rel, world_rel}` with `pre`
   as an operator.

4. **`Key(Type)` earns its place against `->`-as-FD** (LANG.md open question 1).
   Every keyed behaviour in this lab, replacement, the delta pair, and the
   conflict check, keys off the column positions alone. Nothing needed a
   functional dependency arrow. One of the two can go, and the receipts favour
   keeping the column marker.
