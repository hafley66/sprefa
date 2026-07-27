# occurrence_identity: ruling R1, engine stamps vs Z-set counts

Lab: `v6/prolog/labs/occurrence_identity.pl` (20 checks, all PASS, exit 0).
Run: `swipl -q -l v6/prolog/labs/occurrence_identity.pl -g go -g halt`
Receipts: `swipl -q -l v6/prolog/labs/occurrence_identity.pl -g report -g halt`

One interpreter, one switch, five settings. The two the ruling is about:

| switch | identity carrier | scope | what crosses the coastline |
|---|---|---|---|
| `a` | `(tick, seq_in_tick)` stamp | rels declared `event_rel` | one delta per new stamp; set diff elsewhere |
| `b` | integer count per row | every rel | set-projection diff (membership) |

Three more exist so the failure modes get measured instead of asserted:
`a_naive` (stamps on every rel, demand crosses per occurrence), `b_naive`
(a count increment crosses), `hybrid` (counts everywhere plus stamps on
`event_rel`s).

Under every setting, **level rules read the set projection** (stamps and counts
erased) and **folds and edge rules read occurrences**. That split is this lab's
decision, graded by `level_view_sees_set_projection_not_occurrences` and
`level_view_over_folded_state_still_retracts`.

## 1. The comparison

| case | A (stamps) | B (counts) | hybrid |
|---|---|---|---|
| (a) two same-tick increments | `counter(clicks,2)`, correct, rule text unchanged | `counter(clicks,1)` with a count-blind rule. `counter(clicks,2)` only if the rule is rewritten to name the count | correct, same as A |
| (b) last write wins, `v2` then `v1` | `latest(cli,v1)`, deterministic | two admissible answers, `v2` and `v1`. The default the interpreter picks is term order, not arrival order | correct, same as A |
| (b') string concat, `a` then `b` | `ab`; reversed arrival gives `ba` | both orders admissible. Worse: `concat_ba` silently returns `ab` | correct, same as A |
| (c) demand-row dedup | survives only because stamps are scoped to event rels. `a_naive` fires the effect twice | survives only because firing is membership. `b_naive` fires twice | survives |
| (d) 3 JSONL lines one tick | stamps `st(1,1..3)` recoverable from the store | store is byte-identical to the shuffled arrival | order kept |
| (e) level rule over the rel | set projection, no flicker on a repeat | set projection, no flicker | set projection, no flicker |

### (a) the merge-lab undercount

The event rel needs **no id column** under either mechanism. merge_family had to
write `increment(Name, EventId)` for two clicks to be two rows at all
(its ambiguity 3); here `increment/1` suffices and both mechanisms see
multiplicity 2 (`occurrence_identity_removes_the_id_column`).

Under A the fold runs once per stamp and `pre` chains across occurrences inside
the tick:

```
tick 1  [+increment(clicks),+increment(clicks),-counter(clicks,0),+counter(clicks,2)]
store    [held(increment(clicks),[st(1,1),st(1,2)],2)]
```

The intermediate `counter(clicks,1)` exists inside the tick and never crosses
the boundary. That is R7's boundary diffing doing its job, measured.

Under B the same rule text undercounts:

```
tick 1  [+increment(clicks),-counter(clicks,0),+counter(clicks,1)]
store    [held(increment(clicks),[],2)]
```

**Exactly what the fold definition must look like under B.** The body has to
name the occurrence count and the step has to be scaled by it:

```
counter(name, next) <+ increment(name) @ count, pre(counter(name, total)),
                       next is total + count;
```

(the lab spells the wrapper `count_of(increment(Name), Count)`; `@ count` is the
surface candidate). Graded by `b_count_aware_fold_reaches_two`. Two properties
of that form matter:

1. It is correct under A as well, because under A every occurrence carries
   count 1 (`count_aware_fold_correct_under_both`, run under `a`, `b`, `hybrid`).
   So the count-aware form is the one rule text that is portable across the
   ruling. If B wins, this becomes the only way to write a scan.
2. It only works when the step is *count-scalable*, that is when applying the
   step N times equals applying a step parameterized by N. `+1` is. String
   concat is not, and neither is anything reading an external value per step.

B is well defined for a whole tick exactly when the step functions of the
tick's **distinct** rows commute. Increment and decrement do, so both admissible
orders agree (`b_counter_fold_is_order_independent`, both give
`counter(clicks,0)`). Two increments are the same row, so `f^N` is unambiguous.
The undefinedness is not about multiplicity at all. It is about distinct rows.

### (b) an order-dependent fold, graded honestly

`latest(key, value) <+ set_value(key, value), pre(latest(key, _))` with `v2`
arriving before `v1`, so arrival order and term order disagree.

A: `latest(cli,v1)`. One answer, and it is the arrival-last one.

B: the lab runs the fold under both orderings B is allowed to pick and gets
`latest(cli,v2)` and `latest(cli,v1)` (`b_lww_has_two_admissible_answers`). The
fold is not a function of the B-state.

String concat is the sharper version because nothing about it commutes:

```
concat_ab arrivals [a, b]   A -> log(main, ab)   B -> log(main, ab)
concat_ba arrivals [b, a]   A -> log(main, ba)   B -> log(main, ab)
```

Both arrival sequences produce a byte-identical B store
(`held(append_line(main,a),[],1), held(append_line(main,b),[],1)`) and different
A results (`b_state_collides_on_distinct_arrival_orders`). That is the loss
stated as a collision rather than as an opinion.

The honest grade on B here is worse than "undefinable". B did not refuse. It
returned `ab` for an arrival sequence of `b` then `a`, silently, because the
implementation's fallback order is term order. **Any B implementation has a
fallback order and it is never the arrival order.** Either the checker rejects
non-commuting folds outright, or the language ships a second silent undercount
in the same shape as the first one.

### (c) dedup interaction

The failure is real under both mechanisms and it is not a property of stamps or
counts. It is a property of what the engine treats as a *new thing*.

```
repeat_demand: stale("repos/cli/cli") in tick 1 and again in tick 2
fetch_demand(Endpoint) <+ stale(Endpoint);
```

| switch | fetch_demand fires |
|---|---|
| `a_naive` (stamps on every rel, per-occurrence firing) | 2 |
| `b_naive` (count increment crosses) | 2 |
| `a` (stamps scoped to event rels, membership firing) | 1 |
| `b` (membership firing) | 1 |
| `hybrid` | 1 |

Graded by `occurrence_firing_breaks_demand_dedup` and
`membership_firing_keeps_demand_dedup`. Two fires is gh-cache.dl's
720-vs-12-calls-per-hour failure coming back, which the consolidation doc
already ruled against under the arrival-tick-salt heading.

**The fix, both mechanisms, is the same shape and it is a declaration, not a
mechanism.** A rel is either an occurrence rel (event stream: stamps and/or
multiplicity, every arrival is a new thing) or a set rel (demand, derived,
keyed state: `INSERT OR IGNORE` semantics, identical content is the same thing).
Under A that declaration decides where stamps live; under B it decides whether a
count increment crosses the coastline. Neither mechanism removes the need for
it. So R1's ruling actually owes **two** decisions, and the second one is shared:

- which identity carrier folds read (the R1 question as posed), and
- which rels carry occurrence identity at all (a new declaration either way).

### (d) the JSONL stream, three lines one tick

A, from the store: `st(1,1)-line(1,'a.ts',alpha)`, `st(1,2)-line(1,'a.ts',beta)`,
`st(1,3)-line(1,'b.ts',gamma)` (`a_preserves_jsonl_arrival_order`). This settles
shell_stream ambiguity 7 in the "a tick is not a single instant for a multi
fill" direction: the tick is one transaction, the arrivals inside it are ordered.

B: the store for the forward arrival and for the shuffled arrival are the same
term (`b_loses_jsonl_arrival_order`). What the language observably loses:

1. Any rule that means "this line came after that line". Line numbering has to
   move into the producer's output, so `sprefa-extract` must emit its own
   `seq` field and every other streaming bind must too. That is the same
   requirement as A's stamp, moved from the engine into every bind, unaudited.
2. `Stream(Item, End)`'s terminal guarantee weakens. `Done` and the last `Line`
   in one tick have no order, so "the count of items at completion" is only
   correct because it is an aggregate. Anything positional breaks.
3. Duplicate lines survive as a count, so B does **not** lose multiplicity here.
   Only order. That is worth being precise about: the JSONL case is an order
   loss, not a data loss.

### (e) level rules over stamped or counted rels

**Decision, implemented and graded: a level rule sees the set projection.**
`seen(Path) <- line(_, Path, _)` over the same line arriving in two consecutive
ticks emits `[[+seen('a.ts')], []]` under all five switches
(`level_view_sees_set_projection_not_occurrences`). Under A the store gained a
row (two stamps) and the level view did not move. Under B the count went 1 to 2
and the level view did not move.

If a level view saw stamps, every repeated arrival would emit `-seen/+seen` and
mint bogus history, which is the exact failure R7 names. If it saw counts, the
same. So the projection is not a preference, it is what R7's contract requires
once occurrence identity exists.

Level views over folded state still retract normally
(`level_view_over_folded_state_still_retracts`): `hot` deltas across three ticks
are `[[], [+hot(clicks)], [-hot(clicks)]]` under `a`, `b`, and `hybrid`.

The cost of this decision is named in ambiguity 10 below: an aggregate that
wants to count occurrences cannot be a level rule.

## 2. Measurements

### Delta shape (what crosses the coastline)

`duplicate_line_one_tick`: 3 arrivals, 2 distinct rows, one tick.

| switch | delta rows for `line/3` | across two ticks (same row twice) |
|---|---|---|
| `a` | 3 | `[[+line], [+line]]` |
| `a_naive` | 3 | `[[+line], [+line]]` |
| `b` | 2 | `[[+line], []]` |
| `b_naive` | 3 | `[[+line], [+line]]` |
| `hybrid` | 3 | `[[+line], [+line]]` |

Graded by `delta_shape_measured_per_mechanism`. Read as write volume: on a rel
where repeats are common, A writes one row per occurrence and B writes one
`UPDATE ... SET count = count + n`. On a rel where repeats are rare (most
extraction output) the two are the same volume and A skips the read-modify-write.

R7 needs one restatement to survive this: **the tick's delta set is a delta
multiset on occurrence rels.** "One tick, one delta set" stays true; the set has
multiplicity.

### Storage cost per row

Measured by `storage_cost_stamps_vs_counts`.

| switch | `line/3` physical rows (3 arrivals, 2 distinct) | extra columns on `line/3` | extra columns on `fetch_demand/1` |
|---|---|---|---|
| `a` | 3 | 2 (`tick`, `seq`) | 0 |
| `b` | 2 | 1 (`count`) | 1 |
| `hybrid` | 3 | 2 (`tick`, `seq`; count is derivable) | 1 |

The comparison that matters is not 2 columns against 1. It is:

- A's extra columns are **zero on every set rel**, which is most of the schema.
  B's count column is on every rel by construction, because a Z-set has no
  concept of a rel that is not counted.
- **Retention forces a tick column into B anyway.** `retention_bound` prunes
  edge-headed rels. A prunes with `DELETE FROM rel WHERE tick < ?`, a range scan
  on an index prefix. A count carries no age, so B cannot prune history at all
  without adding a tick column. Once B has a tick column, the marginal cost of A
  on an event rel is **one integer, the seq**, not two.

### What count-IVM gives B natively, and what A must add

The rust store already counts, so B's count column is not new storage in the
engine, it is the existing support count reused. That is B's strongest argument
and it should be stated at full strength.

It is also where B's sharpest hazard lives. **The IVM support count and the
occurrence multiplicity are two different integers that DBSP-style Z-sets
conflate.** Support count answers "how many derivations currently justify this
row" and it must go down when a derivation is retracted. Occurrence multiplicity
answers "how many times did this happen" and it must never go down, because
occurrences cannot un-happen. On a derived set rel they coincide. On an event
rel they do not, and a retraction upstream of an event rel would silently
decrement history. Numbered as ambiguity 1.

What A must add on top of count-IVM: two integer columns on event rels, an
engine-side per-tick sequence counter, and index changes so the primary key on a
stamped rel includes the stamp (which is also what makes `INSERT OR IGNORE`
stop deduping, hence the scoping requirement in (c)).

### Retention interaction

| | A | B |
|---|---|---|
| prune old history | `DELETE FROM rel WHERE tick < ?`, index range scan | no age on a count; needs a tick column added, at which point it is A minus the seq |
| prune by count | not applicable | decrement, but the decrement has to know which occurrences it is dropping, and it cannot |
| bound a stream to last N | `ORDER BY tick, seq LIMIT` | expressible only if the rel also carries something orderable |

`retention_bound` is a requirement per the consolidation doc, not an
optimization. This is the case where A is not merely more expressive, it is the
only one of the two that can implement the required feature without importing
half of the other.

### Which v5 behaviors each mechanism reproduces

| v5 behavior | A | B |
|---|---|---|
| `etag(ep, tag) <- @next etag_next(ep, tag)` (gh-cache.dl:104), a keyed latest-wins carry | yes, keyed fold, one occurrence per tick | yes |
| `change_log(ep, kind, val) <- @next change_log_next(...)` (gh-cache.dl:137), append-only union with structural dedup | yes, and *more*: with `change_log` declared an event rel, stars going 42, 43, 42 records three occurrences | yes, exactly, including the defect |

The second row is worth reading twice. v5's `change_log` comment says dedup is
structural and re-deriving an unchanged entity is a no-op, "the ghcacher
INSERT-OR-IGNORE property". That is correct for idempotence and it also means
**v5's change feed cannot record a value returning to a previous value**, and
its SSE tail has no defined order beyond whatever rowid happens to give. B
reproduces v5 exactly, defect included. A reproduces it and can also express the
fixed version. Neither result is an argument on its own, because faithful
reproduction of v5 is not a goal; it is a data point on which mechanism can
express the superset.

## 3. The hybrid

The two pure mechanisms fail different cases, so the third column is licensed:
B fails (b) and (d), A fails (c) only in its unscoped form, which is a scoping
choice rather than a property of stamps.

**Hybrid, stated precisely:** every rel carries the count column, which is the
count-IVM support count the rust store already maintains. A rel declared an
event stream *additionally* carries `(tick, seq_in_tick)`, its rows are distinct
by stamp, and folds over it run per stamp in stamp order. Non-event rels are
sets at the coastline: identical content does not cross twice, so
content-addressed demand dedup is untouched. Level rules read the set projection
on both kinds.

Graded by `hybrid_settles_undercount_order_and_dedup`: `counter(clicks,2)`,
`latest(cli,v1)`, `log(main,ab)`, and exactly one `fetch_demand` fire. The count
column survives underneath the stamps
(`multiplicity(line(1,'a.ts',alpha)) = 2` alongside `st(1,1)` and `st(1,2)`).

## 4. Recommendation (ADVISORY, the ruling is the user's)

**Hybrid.** Ranked reasons, strongest first:

1. B has no answer for order-dependent folds and does not fail loudly. The
   `concat_ba` receipt returns `ab`. A silent wrong answer in the same shape as
   the bug R1 was opened to fix is a bad trade.
2. Retention is a requirement and drags a tick column into B regardless, so the
   real marginal cost of stamps over counts on an event rel is one integer.
3. Count-IVM is already built and B's count column is free. Throwing it away for
   pure A costs the engine its existing support counting, and gains nothing
   stamps do not already give.
4. The scoping declaration (which rels are event streams) is needed under both
   mechanisms, so it is not a cost the hybrid uniquely pays.

Ranked against the alternatives: pure B is acceptable **only** if the checker
rejects every non-commuting fold, which rules out last-write-wins-on-a-stream
and any string or list accumulation, and which needs a commutativity proof
obligation nobody has scoped. Pure A is acceptable and simpler to explain, and
its cost is discarding the store's existing count column and re-deriving support
some other way.

The one thing that would flip this: if the surface decides no fold may be
order-dependent (last-write-wins is only ever the *key's* semantics across
ticks, never a fold within one), then B's failure case (b) disappears and B wins
on simplicity. That is a language question, not a mechanism question, and it is
the question the ruling should be argued on.

## 5. What each choice does to R2, R7, and register_lowering

### R2 (`<+` into a keyed rel)

Occurrence identity supports R2's stated resolution direction and adds a
receipt. Under A the keyed head is replaced once per occurrence inside the tick
and only the last value crosses:
`tick 1 [-counter(clicks,0),+counter(clicks,2)]`. The intermediate
`counter(clicks,1)` is real, it exists between two occurrences, and the boundary
diff erases it. So "the arrow owns the trigger, the key owns the storage effect"
is exactly right, and the restatement needs one more clause: *the intermediate
per-occurrence states of a keyed edge head are not observable at the tick
boundary.* Under B there are no intermediate states at all, so R2's restatement
is shorter but the language is weaker.

Second effect: R1 dissolves merge_family's same-tick keyed conflict for folds.
`mixed_inc_dec_one_tick` was a rejected program under merge_family's conflict
law (one increment and one decrement, one key, one tick). Under A it is ordered
and gives 0; under B it commutes and gives 0. The conflict law still has work to
do for non-fold keyed heads, but the fold case leaves its scope.

### R7 (tick-boundary diffing)

Both mechanisms satisfy R7 and both need the same one-line restatement: the
delta set is a delta **multiset** on occurrence rels. Everything else in R7
stands, and this lab's level-projection decision is what keeps it standing (a
level view that saw stamps would flicker on every repeated arrival, which is the
exact failure R7 names).

### register_lowering

The consolidation doc has `register_lowering` blocked on R1. Here is what each
answer unblocks it into.

Under B, a fold lowers to one set-based statement per keyed rel:

```sql
UPDATE counter SET total = total +
  (SELECT SUM(count) FROM increment_delta WHERE name = counter.name);
```

One statement, no loop, no window function. It only exists for count-scalable
steps, so the checker owes a commutativity and count-scalability proof per fold,
and it must reject the rest.

Under A, a fold lowers to one statement per keyed rel using window functions
over the tick's arrival table ordered by `seq`:

| fold | lowering |
|---|---|
| accumulate | `SUM(delta) OVER (PARTITION BY key ORDER BY seq)`, take the last |
| last write wins | `ROW_NUMBER() OVER (PARTITION BY key ORDER BY seq DESC) = 1` |
| concat | `group_concat(piece ORDER BY seq)` (sqlite 3.44+) |

Also one statement, no driver loop, and it covers the non-commuting folds. The
checker owes nothing extra. This is a stronger practical argument for A than the
semantics section, and it is the one that should be verified against the actual
sqlite version pinned in the store before the ruling is taken as settled.

The `UPDATE ... CASE` shape the consolidation doc worried about is not needed
under either answer. What both need is that the tick's arrivals are addressable
as a table, which is a lowering requirement neither mechanism states today.

Under the hybrid, `register_lowering` picks per rel: window-function lowering on
event-declared rels, aggregate lowering elsewhere. That is two code paths, and
it is the hybrid's honest cost.

## 6. Numbered new ambiguities

1. **Z-set multiplicity and IVM support count are the same integer and must not
   be.** Support count goes down when a derivation is retracted. Occurrence
   multiplicity must not, because occurrences cannot un-happen. On derived set
   rels they coincide; on event rels they do not. If B wins, the store needs two
   count columns or a rule saying event rels are exempt from support decrement.
2. **Which rels are occurrence rels has no surface syntax.** Both mechanisms
   need it (section (c)). Candidates: a keyword on the `rel` declaration, or
   inference from "this rel is only ever headed by `<+`", or inference from
   "this rel is filled by a bind". Inference is attractive and untested.
3. **Does `pre` chain across occurrences inside a tick?** This lab says yes and
   that is what makes the fold correct. merge_family ambiguity 5 says every arm
   reads the same `pre` (T-1), which is what causes the undercount. R1's ruling
   forces an answer here, and it is a different sentence from the one R1 states.
4. **Intermediate per-occurrence values of a keyed rel: guaranteed invisible, or
   accidentally invisible?** A downstream edge rule triggered on
   `counter(clicks, _)` never sees `counter(clicks,1)` in the two-increment tick.
   Whether that is a promise or an artifact of diffing at the boundary is
   unstated, and it is the difference between a fold being atomic and a fold
   being a sequence of visible steps.
5. **A fold whose drivers are partly stamped has only a partial order.** This lab
   drops the entire fold to counted mode when any driver lacks stamps. The
   alternatives (reject the program, or order stamped occurrences before
   unstamped ones) are both defensible and none is written down.
6. **Retention needs a tick column under B.** Section 2. If B wins, either
   retention is dropped as a requirement or B quietly becomes A minus the seq.
7. **The count-aware body form has no syntax.** `count_of(atom, Count)` is this
   lab's encoding; `atom @ count` is the surface candidate. If B wins this is a
   new grammar production in every scan, and `surface_dcg` owes it. If the count
   is instead an implicit variable, that is a second implicit time-like
   coordinate alongside the phantom tick.
8. **Is `seq` global per tick or per rel?** This lab assigns one counter across
   all arrivals in the tick, which is why an increment and a decrement arriving
   together are ordered relative to each other. A per-rel seq would leave
   cross-rel fold arms unordered, which puts back most of B's problem.
9. **A stamp is receive order, not world order.** Two binds delivering in one
   tick are ordered by whichever the engine drained first. The stamp promises
   determinism given a fixed delivery, not truth about the outside world. That
   distinction has to be in the docs or every user will read it wrong.
10. **A level rule cannot count occurrences.** The set-projection decision means
    `count(*)` over an event rel through a level rule returns the number of
    distinct rows, not the number of events. Aggregates therefore need
    occurrence access, which is a second read mode on the same rel, and the
    aggregate surface does not exist yet (audit: 76 v5 files use aggregates).
11. **What does retracting an event row mean?** Under A, does it remove one stamp
    or all of them? Under B, decrement by one or to zero? Nothing retracts event
    rows in this lab, so the question is open and it is the same question as
    ambiguity 1 seen from the rule side.

## 7. Deviations from the LANG.md snapshot

1. Rules are prolog terms under `op(1150, xfx, <-)` and `op(1150, xfx, <+)`,
   read by a `term_expansion` hook. The interpreter shape is copied from
   `merge_family.pl`, not imported, per the brief.
2. Two declarations LANG.md does not have: `event_rel(Ref)` (which rels carry
   occurrence identity, see ambiguity 2) and `count_of(Atom, Count)` (the
   count-aware body form, ambiguity 7). Both exist because the comparison cannot
   be run without them.
3. Plain edge rules in this lab have a single-atom body. Multi-atom edge bodies
   and their any-atom trigger shape are merge_family's and check_eventing's
   ground and are not re-tested here.
4. Arrivals reach a tick as an ordered list with duplicates allowed, rather than
   as a set of deltas. That is the input the mechanisms disagree about, so it
   cannot be normalized before the switch.
5. Fold rules are edge rules whose body reads `pre` of their own head. LANG.md
   has no fold construct; this is merge_family's mergeByKeyScan shape.
6. Nothing in this lab retracts a store row, so the retraction half of R1
   (ambiguity 11) is untested.

## 8. What this means for the tier order

- `register_lowering` unblocks under either answer, into two different SQL
  shapes (section 5). The window-function route should be verified against the
  pinned sqlite version before the ruling is taken as final, because
  `group_concat(... ORDER BY ...)` needs 3.44.
- `retention_bound` moves from requirement to **gate on R1**: it is
  implementable directly under A and needs a schema addition under B.
- `count_ivm_port` gains ambiguity 1 as a blocking question. The port cannot
  reuse the support count as the occurrence count without deciding what a
  retraction does to history.
- `mode_lab` gains nothing here. Occurrence identity is orthogonal to
  cardinality and lifetime; a `(multi, finite)` stream is stamped or counted the
  same way a det fill is.
- The aggregate surface (ambiguity 10) now has a hard dependency on R1, because
  "count the occurrences" and "count the distinct rows" become different
  questions the moment either mechanism lands. The audit already put aggregates
  in the missing 90%; this adds a constraint on what they have to be able to say.
