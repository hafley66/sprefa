# CONSUMPTION + ARMS: lab verdict (2026-07-28)

Contract: `plans/2026-07-28-consumption-arms-lab-header.md`.
Lab: `v6/prolog/labs/consumption_arms/` (10 files, entry `lab.pl`, **90 PASS**,
exit 0, stdout is PASS lines only, stderr empty).
Untouched and re-run to prove it: conformance `go.pl` **110 PASS**,
`v6/prolog/compile/scripts/roundtrip.sh` **ALL GRADES PASS** (G1 110/110,
G2 no parse errors, G3 110/0).
Rounds: **7**, fixpoint closed at round 7 with zero findings.
Assertions: **28**, of which 5 were amended by a later round.

---

## VERDICT LINE

**The consumption axis needs no construct at all: switch and queue are the
same rel written with two different key declarations, and every arm in the
ruled Observer vocabulary is row granularity on some rel. Three things break.
Pacing (a) destroys the queue it implements. A durable pending rel does not
make a firing durable. And a keyed envelope silently swallows an error before
the error arm can see it, which is the case that makes the ruled collapse
trace the only thing standing between a dropped failure and total silence.**

More precisely:

- **HOLDS, thread 2.** `switch` is `key(key)`. `queue` is
  `key(queue, ordinal)` plus a min-ordinal level view plus a `done` rel, the
  shape `fixtures/scopes.pl:146` already ships. The ordinal mints in the
  language today through a keyed counter read via `pre/1`, which chains
  across occurrences inside one tick. No new construct, no exposed engine
  stamp column.
- **HOLDS, thread 1.** All six arm words ground out to shipped kernel forms.
  `subscribe` and `unsubscribe` are `next` and `finalize` on the demand rel;
  `complete` is `finalize` on the live scope rel; `error` is an ordinary enum
  variant destructure. Zero constructs for the whole vocabulary.
- **HOLDS, thread 3.** A channel with N readers and M writers composes out of
  a Log rel, a keyed cursor rel, an arithmetic guard and a `min` aggregate.
  The low watermark is an ordinary aggregate over the cursor rel.
- **BREAKS 1: pacing (a) is not a queue.** Landing every queued item in one
  tick puts N writes on one key at any keyed consumer, which fold to one. In
  the graded run 2 of 3 items vanish, and the survivor is chosen by the
  standard order of the ready view's TERMS: move the payload column ahead of
  the ordinal column and ordinal 1 beats ordinal 3. Pacing (a) preserves no
  order the program stated.
- **BREAKS 2: the C7 sidestep is conditional.** Restarting from the durable
  queue produces **zero ticks**. The rows are intact and the min-ordinal head
  is recomputed and present in the final state; no occurrence is ever minted
  for it. Re-delivering the rows as arrivals produces nothing either, because
  an already-present Set row is not an occurrence. The row survives the
  crash. The firing does not.
- **BREAKS 3: a keyed envelope swallows errors.** An error row and a later ok
  row for one key in ONE tick leave `handled` unfired and no rel anywhere
  recording that a failure happened. The same two rows one tick apart do fire
  it. Whether a failure is observed is a function of scheduler batching.
- **NAMED GAP, thread 3.** `keep(count(N))` is a function of the log alone
  and channel retention is a function of a join, so no static bound expresses
  it. Worse than inexpressive: a row appended and pruned inside one tick
  carries no delta of any sign anywhere in the run.
- **BOUND, thread 5.** The level-as-signed-edge desugar agrees on every delta
  of the plus half and is inexpressible on the minus half, because no edge
  head retracts. rx CAN express it (a `scan` owning a set it may shrink),
  which locates the gap in the kernel rather than in the idea.

---

## THE ASSERTION SET

28 assertions. Each names its minting round and the lab checks that validate
it. These are the future fixtures and rulings. `AMENDED` marks an assertion a
later round corrected; the text below is always the corrected text.

### Thread 2, the consumption axis

| # | round | assertion | validating checks |
|---|---|---|---|
| 1 | 1 | The consumption axis is spelled by the key declaration and nothing else. `switch` is `key(key)`; `queue` is `key(queue, ordinal)` plus a min-ordinal level view plus a `done` rel. Neither side needs a construct. | `r1_switch_is_the_key_declaration`, `r1_queue_is_the_ordinal_key_declaration` |
| 2 | 1 | The queue ordinal is mintable in the language today: a keyed counter read through `pre/1` chains across occurrences inside one tick, so three pushes in one tick get 1, 2, 3. No engine stamp column has to be exposed. | `r1_ordinal_mints_in_language_across_one_tick`, `r5_duplicate_log_pushes_in_one_tick_get_distinct_ordinals` |
| 3 | 1 | Pacing (a) costs one drain tick whatever the queue length; pacing (b) costs one per item, so a queue of N settles in N plus two ticks. | `r1_pacing_a_lands_every_item_in_one_tick`, `r1_pacing_b_lands_one_item_per_drain_tick`, `r3_pacing_b_drain_count_is_queue_length_plus_two`, `r3_pacing_a_drain_count_is_flat_in_queue_length` |
| 4 | 2 **AMENDED r3** | Into a Log consumer the two pacings deliver identical rows and differ only in the tick index. Into a KEYED consumer pacing (a) loses N minus 1 of N items. | `r1_both_pacings_deliver_the_same_rows_into_a_log_consumer`, `r2_pacing_a_loses_two_of_three_items_at_a_keyed_consumer`, `r2_pacing_b_keeps_all_three_items_at_a_keyed_consumer` |
| 5 | 2 | Under pacing (a) the survivor is not the last item queued: the within-tick fold order is the standard order of the ready view TERMS, so a column swap changes the winner. | `r2_pacing_a_survivor_is_decided_by_column_order_not_by_ordinal` |
| 6 | 3 | A durable pending rel does not on its own sidestep the non-durable Ti carry. After a restart the row is intact and the head is recomputed and present, and the run produces zero ticks. | `r3_crash_restart_stalls_with_the_queue_intact`, `r4_a_partially_drained_queue_also_stalls_on_restart` |
| 7 | 3 | Re-delivering the durable rows as arrivals does not restart the queue, because an already-present Set row is not an occurrence. Restart needs a boot occurrence policy, not a replay. | `r3_replaying_the_durable_rows_as_arrivals_also_stalls`, `r3_a_genuinely_fresh_arrival_drains_the_whole_queue` |
| 8 | 3 | Under pacing (b) the drain cap becomes a data-dependent queue-length cap: 99 items settle in 101 ticks, 100 items throw `drain_overflow(100)`. Under pacing (a) the same 100 items settle in 3 ticks. | `r3_pacing_b_of_ninety_nine_items_survives_the_cap`, `r3_pacing_b_of_one_hundred_items_throws_drain_overflow`, `r3_pacing_a_of_one_hundred_items_does_not_throw` |

### Thread 1, the lifecycle arms

| # | round | assertion | validating checks |
|---|---|---|---|
| 9 | 1 | `subscribe` and `unsubscribe` are `next` and `finalize` on the DEMAND rel; `complete` is `finalize` on the LIVE SCOPE rel. All three ground out to shipped kernel forms with no construct. | `r1_subscribe_and_unsubscribe_are_next_and_finalize_on_the_demand_rel`, `r1_complete_is_finalize_on_the_live_scope_rel`, `r1_next_fires_in_the_arrival_tick_and_finalize_one_drain_later` |
| 10 | 3 | Every arm in the ruled vocabulary is ROW granularity once the right rel is named. There is no rel-level arm, which is what makes the family one construct rather than two. | `r3_every_arm_is_row_granularity_on_some_rel` |
| 11 | 3 | Three of the six arms fire on a rel the arm does not name. That mismatch is the whole content of `SLOT-ARM-ARGUMENT`. | `r3_three_arms_fire_on_a_rel_they_do_not_name` |
| 12 | 1 **AMENDED r6** | The error arm survives only as an enum variant destructure over the envelope rel, and only over a LOG envelope. The second-channel reading is refused on three independent grounds. | `r1_the_error_arm_is_an_ordinary_variant_destructure`, `r2_the_second_channel_reading_is_refused_on_three_grounds`, `r6_a_log_envelope_never_swallows_an_error` |
| 13 | 2 **AMENDED r6** | The error arm is not terminal and is not even guaranteed to fire. The rel keeps producing next rows after it; on a keyed envelope an error row arriving with a later ok row is replaced before any arm sees it. | `r2_the_rel_keeps_producing_after_the_error_arm_fires`, `r6_a_keyed_envelope_swallows_an_error_delivered_in_the_same_tick`, `r6_the_same_two_rows_one_tick_apart_do_fire_the_error_arm` |
| 14 | 2 | An error arm over a rel whose decl declares no error variant loads, runs and never fires, with no diagnostic. Same class as `finalize` over a Log rel. | `r2_an_error_arm_with_no_matching_variant_is_silently_dead` |
| 15 | 3 | The arm family is not timing symmetric: `next`, `subscribe`, `error` fire in the tick of their plus delta; `finalize`, `unsubscribe`, `complete` fire one drain tick after their minus delta, because a minus delta only becomes an occurrence through the departure carry. | `r3_plus_side_arms_fire_at_t_and_minus_side_arms_at_t_plus_one`, `r1_next_fires_in_the_arrival_tick_and_finalize_one_drain_later` |
| 27 | 6 | Whether an error variant is observed at all depends on the envelope key declaration and on scheduler batching. The ruled collapse event is the only mechanism anywhere that reports the drop. | `r6_a_keyed_envelope_swallows_an_error_delivered_in_the_same_tick`, `r6_the_same_two_rows_one_tick_apart_do_fire_the_error_arm`, `r6_the_collapse_event_is_what_reports_the_swallowed_error`, `r6_a_log_envelope_never_swallows_an_error` |

### Thread 3, the channel

| # | round | assertion | validating checks |
|---|---|---|---|
| 16 | 1 | A channel with N readers and M writers composes today out of a Log rel, a keyed cursor rel, an arithmetic guard and a `min` aggregate. M writers in one tick get distinct ordinals; readers advance independently one ordinal per tick; a late reader catches up from its own cursor. | `r1_two_writers_in_one_tick_get_distinct_ordinals`, `r1_two_readers_advance_independently_and_the_watermark_follows`, `r1_a_late_reader_catches_up_from_its_own_cursor` |
| 17 | 2 **AMENDED r5** | `keep(count(N))` prunes with no delta of any kind: the tick logs of the same program under `keep(all)` and `keep(count(2))` are identical while the final states differ by a row. | `r2_the_prune_is_invisible_in_the_tick_log`, `r3_retention_already_removes_log_rows_without_a_delta`, `r5_keep_all_shows_the_row_that_keep_count_one_erased` |
| 18 | 2 | A static `keep(count(N))` permanently stalls a lagging reader: its cursor never moves again, the run goes quiescent with three empty ticks, and the watermark that would have prevented the prune sits in the final state saying the pruned ordinal was unread. | `r2_a_static_keep_count_permanently_stalls_the_lagging_reader`, `r2_the_same_program_under_keep_all_loses_nothing` |
| 19 | 3 | `keep(count(N))` is a function of the log alone and channel retention is a function of a JOIN, so no static bound expresses it. The smallest honest spelling is retention as an ordinary rule. | `r3_every_retention_option_is_priced_both_ways`, `r3_retention_already_removes_log_rows_without_a_delta`, `r3_explicit_log_retraction_throws_today` |
| 26 | 5 | A Log row appended and pruned inside ONE tick carries no delta of any sign anywhere in the run. Retention can erase a row from the grading record entirely. | `r5_a_row_appended_and_pruned_in_one_tick_has_no_delta_of_any_sign`, `r5_keep_all_shows_the_row_that_keep_count_one_erased` |

### Thread 4, transition collapse logging

| # | round | assertion | validating checks |
|---|---|---|---|
| 20 | 1 | Exactly one instrumentation site is reachable: the keyed store write. Every frontier the ruling names funnels through it, and duplicate Set adds, Log appends and level recomputes cannot collapse at all. | `r1_exactly_one_instrumentation_site_is_reachable`, `r1_a_duplicate_set_add_is_not_a_collapse`, `r1_a_log_append_is_not_a_collapse`, `r4_a_collapse_inside_a_drain_tick_mints_one_event_from_the_same_site` |
| 21 | 1 | The count is WRITES per key per tick, one event per key, not one event per lost intermediate. | `r1_two_writes_one_key_one_tick_mints_one_event`, `r1_three_writes_mint_one_event_counting_writes_not_intermediates`, `r1_one_write_per_key_mints_no_event`, `r5_two_rules_writing_one_key_in_one_occurrence_throw_keyed_conflict` |
| 22 | 2 | The event must fire on write count and NOT on delta presence. A net-zero pair leaves the boundary showing nothing at all, and that is the case where silence is most misleading. An equal-row rewrite counts too. | `r2_a_net_zero_pair_of_writes_shows_no_delta_at_all`, `r2_the_event_still_fires_on_the_net_zero_pair`, `r2_an_equal_row_rewrite_still_counts_as_a_collapsed_write` |
| 23 | 3 | A trace-only collapse event is not conformance-checkable: two runs with different collapse counts produce byte-identical tick logs for the collapsed rel. `SLOT-COLLAPSE-CHANNEL`. | `r3_two_runs_with_different_collapse_counts_share_a_tick_log`, `r3_the_grading_gap_is_a_named_slot` |

### Thread 5, level rule as signed edge

| # | round | assertion | validating checks |
|---|---|---|---|
| 24 | 1 **AMENDED r4** | The desugar agrees on every DELTA of the plus half: same tick, same order, same rows. It does not agree byte for byte on the whole log. | `r1_the_plus_half_lands_the_same_deltas_in_the_arrival_tick` |
| 25 | 2 | The desugar is inexpressible on the minus half. Edge heads only append or replace; a `finalize` arm observes the departure but writes a NEW row one drain tick later. rx can express it, which locates the gap in the kernel. | `r2_the_level_form_retracts_in_the_departure_tick`, `r2_the_edge_form_never_retracts_at_all`, `r2_a_finalize_arm_writes_a_row_and_cannot_remove_one`, `r3_the_claim_holds_on_the_plus_half_and_fails_on_the_minus_half` |
| 28 | 4 | The edge form mints one trailing quiescence tick the level form never mints. It comes from the edge WRITE carrying itself, not from the head kind, which is why a level rel feeding an edge rule mints the same tick. | `r4_the_edge_form_mints_one_extra_quiescence_tick`, `r4_the_quiescence_tick_comes_from_the_edge_write_not_the_head_kind` |

---

## THE PACING COMPARISON, both ways with real logs

One program, one schedule: three requests arriving in ONE tick.

### The two spellings

```
rel req(value: text) log keep(all)
rel counter(queue: text, next: int) key(queue)
rel slot(queue: text, ordinal: int, value: text) key(queue, ordinal)
rel done(queue: text, ordinal: int) key(queue, ordinal)

counter(queue, next)     <+ req(value), pre(counter(queue, so_far)), next := so_far + 1.
slot(queue, next, value) <+ req(value), pre(counter(queue, so_far)), next := so_far + 1.

% pacing (b): the min-ordinal head
head(queue, min(ordinal))          <- slot(queue, ordinal, _), not(done(queue, ordinal)).
head_value(queue, ordinal, value)  <- head(queue, ordinal), slot(queue, ordinal, value).

% pacing (a): every undrained slot at once
head_value(queue, ordinal, value)  <- slot(queue, ordinal, value), not(done(queue, ordinal)).

done(queue, ordinal) <+ head_value(queue, ordinal, _).
out(value)           <+ head_value(queue, _, value).
```

rx lowering, pacing (b):

```js
req$.pipe(
  scan((slot, value) => ({ ordinal: slot.ordinal + 1, value }), { ordinal: 0, value: null }),
  concatMap(slot => consumeOne(slot)))
```

rx lowering, pacing (a): the same pipeline with `mergeMap` in place of
`concatMap`. `concatMap` IS the min-ordinal drain: it subscribes to the next
inner only after the previous inner completes, which is exactly one item per
settled boundary.

### Log consumer, pacing (a): 3 ticks

```
1: [-counter(q,0),+counter(q,3),
    +head_value(q,1,a),+head_value(q,2,b),+head_value(q,3,c),
    +slot(q,1,a),+slot(q,2,b),+slot(q,3,c),
    +req(a),+req(b),+req(c)]
2: [-head_value(q,1,a),-head_value(q,2,b),-head_value(q,3,c),
    +done(q,1),+done(q,2),+done(q,3),
    +out(a),+out(b),+out(c)]
3: []
```

### Log consumer, pacing (b): 5 ticks

```
1: [-counter(q,0),+counter(q,3),+head(q,1),+head_value(q,1,a),
    +slot(q,1,a),+slot(q,2,b),+slot(q,3,c),
    +req(a),+req(b),+req(c)]
2: [-head(q,1),-head_value(q,1,a),+done(q,1),+head(q,2),+head_value(q,2,b),+out(a)]
3: [-head(q,2),-head_value(q,2,b),+done(q,2),+head(q,3),+head_value(q,3,c),+out(b)]
4: [-head(q,3),-head_value(q,3,c),+done(q,3),+out(c)]
5: []
```

Into a Log consumer the two are the same three rows at different tick
indices. That is the whole difference, and it looks like a cost question.

### Keyed consumer `seen(queue, value) key(queue)`, pacing (a)

```
2: [-head_value(q,1,a),-head_value(q,2,b),-head_value(q,3,c),
    +done(q,1),+done(q,2),+done(q,3),+seen(q,c)]
final: seen(q,c) only. seen(q,a) and seen(q,b) never existed.
```

### Keyed consumer, pacing (b)

```
2: [-head(q,1),-head_value(q,1,a),+done(q,1),+head(q,2),+seen(q,a),+head_value(q,2,b)]
3: [-head(q,2),-seen(q,a),-head_value(q,2,b),+done(q,2),+head(q,3),+seen(q,b),+head_value(q,3,c)]
4: [-head(q,3),-seen(q,b),-head_value(q,3,c),+done(q,3),+seen(q,c)]
```

Three distinct boundary transitions, `a` then `b` then `c`, in order.

### The column-order receipt

Same pacing (a) program with the ready view written
`ready(queue, value, ordinal)` instead of `head_value(queue, ordinal, value)`,
fed `zulu`, `alpha`, `mike` in that order:

```
final: seen(q,zulu), and slot(q,1,zulu)
```

The survivor is ordinal **1**, not ordinal 3, because the within-tick fold
runs in standard order of the ready view's terms and `zulu` sorts last. With
the ordinal column first the survivor is ordinal 3. The queue's own order
never enters the decision.

### What downstream observes differently

| | pacing (a) | pacing (b) |
|---|---|---|
| ticks for N items | 3, flat | N plus 2 |
| rows at a Log consumer | N | N |
| rows at a keyed consumer | 1 | N |
| survivor at a keyed consumer | decided by term order of the ready view | the last item, in queue order |
| sequential dependence between items | not expressible, all items see one state | each item sees the previous item's output |
| drain cap interaction | never approached | queue depth capped at 99, `drain_overflow(100)` at 100 |
| collapse events minted | one per key per tick | none |

### Which composes with the arms model without a new construct

Both do. Neither pacing needs anything the language lacks: (a) is a plain
level rule, (b) is a level rule with a `min` aggregate. The question is not
expressibility.

Reading, offered as a reading and not a ruling: **pacing (b) is the only one
that implements a queue.** Pacing (a) is a batch view wearing a queue's
spelling. It reintroduces the exact collapse the queue existed to avoid, at
the first keyed consumer downstream, and it does so with a survivor the
program did not choose. Its one advantage is real and quantified: it never
approaches the drain cap, where pacing (b) hard-fails at 100 queued items
under the standing `SLOT-SPILL` ruling (error at cap, never spill).

That leaves a live cost with an owner: **pacing (b) needs the drain cap to
stop being a queue-length cap.** Options, unpriced here because they belong to
the scheduler arc: a per-rel drain allowance, a cap counted in work rather
than ticks, or a documented maximum queue depth.

---

## THE ERROR-ARM RESOLUTION

The contract asked for this graded, not assumed. Three readings were tested.

### Refused: the error arm as a second failure channel

`arm_refusal(error, second_failure_channel(error), ...)`. Three independent
grounds, each a standing law rather than a lab opinion:

1. the failure-is-a-value envelope ruling bans a second failure channel
   outright;
2. an exception is not a row, so it has no delta shape;
3. a non-row can never appear in the tick log, so the item-9 grading currency
   could never see it, and two runners could disagree about failures while
   both grade PASS.

### Survives: the error arm as an enum variant destructure

```
rel resp(key: text, value: ok(body: text) ; error(message: text)) log keep(all)
rel served(key: text, body: text) log keep(all)
rel handled(key: text, message: text) log keep(all)

served(key, body)     <+ resp(key, ok(body)).
handled(key, message) <+ resp(key, error(message)).
```

rx lowering:

```js
const resp$ = source$.pipe(shareReplay({ refCount: false }));
const served$  = resp$.pipe(filter(row => row.value.tag === 'ok'),
                            map(row => ({ key: row.key, body: row.value.body })));
const handled$ = resp$.pipe(filter(row => row.value.tag === 'error'),
                            map(row => ({ key: row.key, message: row.value.message })));
```

Note what the lowering is NOT: it is not `catchError`, and it is not the
error channel of `subscribe`. It is `filter` over a tagged value on the next
channel, which is precisely what the envelope ruling says failure is.

### The three costs the word carries

1. **It is not terminal.** rx `error` is the last notification a subscription
   ever receives. Here the rel keeps producing in the very next tick:
   `resp(a, ok(one))`, `resp(a, error(boom))`, `resp(a, ok(two))` fires
   `served`, `handled`, `served` in three consecutive ticks with nothing
   marking anything dead.
2. **It is not guaranteed to fire.** On a keyed envelope, which is the shape
   every stale-while-revalidate cache uses:

   ```
   1: [+latest_resp(a,ok(two)),+resp(a,error(boom)),+resp(a,ok(two))]
   2: [+served(a,two)]
   3: []
   final: no handled(a,boom) anywhere
   ```

   The same two rows one tick apart do fire `handled`. Whether a failure is
   observed is a function of scheduler batching.
3. **It is silently dead over a rel with no error variant.** The rule loads,
   runs, and never fires, the same class as `finalize` over a Log rel.

### The resolution

**Reading (A) only, with three obligations.** `error` is sugar for a variant
arm and nothing else. It never becomes a channel. The obligations are named,
not assumed:

- `SLOT-ERROR-VARIANT-NAME`: under the enum ruling a variant is named by the
  program. Either `error` is a reserved variant name, or the arm word is
  dropped and the program writes the variant arm by its own name.
- `SLOT-ERROR-TERMINALITY`: either the word is kept and the difference from
  rx is written down loudly, or the arm additionally retracts the demand row,
  which would make `error` the only arm with a side effect.
- **The keyed-envelope swallow is a diagnosis obligation, not a semantics
  one.** The collapse trace already ruled in `transition_rule_semantics`
  reports it exactly: `collapse(1, latest_resp/2, [a], 2, true)`. This is the
  first place in the lab where the trace obligation is the only thing between
  a dropped failure and silence, and it is the strongest argument found for
  the ruling.

---

## THE WATERMARK SLOT

`SLOT-RETENTION-SPELLING`. A proposal, not a fiat: four options priced both
ways, the smallest honest one named.

### What `keep(count(N))` cannot express, precisely

`keep(count(N))` is a function of the log alone: newest N stamps, evaluated at
tick end. Channel retention is a function of a JOIN against the reader
cursors. No static bound expresses a join. Two graded consequences:

1. **the failure is not graceful.** With `keep(count(2))` and a reader that
   wakes at tick 4, the reader is permanently stalled: cursor frozen at 0,
   watermark frozen at 0, three empty ticks, no diagnostic. The watermark that
   would have prevented the prune sits in the final state saying ordinal 1 was
   unread.
2. **the prune is invisible, and can be total.** The tick logs of the same
   program under `keep(all)` and `keep(count(2))` are byte identical. Worse:
   a row appended and pruned inside ONE tick has no delta of any sign
   anywhere in the run. `chan(1, m1)` is written, retained out, and never
   appears in the tick log at all.

### The four options

| option | spelling | buys | costs |
|---|---|---|---|
| **s1** | retention as an ordinary rule: a retracting head over the log rel, `chan(ordinal, _)` leaves when `ordinal =< watermark` | zero new decl words; the prune becomes a visible minus delta, which closes the silent-prune crack in the same change; the bound is an ordinary derived value so any join expresses it; tick-log-only grading can see retention for the first time | requires lifting `retract_from_log`, which throws today; a retracting head is a new head kind for edge rules and none exists; stratification obligation so the retention read does not feed the log it prunes |
| s2 | `keep(until(watermark, last))` in the decl | no change to the append-only law; the bound stays where every other retention bound lives | one new decl word; the prune stays invisible; a decl now names a rel, making decls order-dependent on rules |
| s3 | `keep(min(cursor.last))` | most general, any aggregate over any rel | puts an expression language inside decls that nothing else needs; same invisibility cost; the aggregate is recomputed at tick end, off the rule plane |
| s4 | no construct: store the channel as a keyed Set rel and delete rows with an ordinary rule | zero language change | loses stamps, so it loses duplicate occurrences and arrival order; a Set rel cannot hold two identical payloads, which a channel must; the program pays bookkeeping the log already does |

### The smallest honest proposal

**s1, retention as an ordinary rule**, on one argument: the append-only law
the ban protects is **already violated by retention itself**, invisibly.
`keep(count(2))` removes a Log row the program can never see leave, and
`keep(count(1))` removes one that never appeared at all. s1 does not
introduce deletion of log rows. It makes the deletion that already happens
visible and programmable.

The .dl surface it would need, written out so the cost is legible:

```
rel chan(ordinal: int, payload: text) log keep(rule)
rel cursor(reader: text, last: int) key(reader)

watermark(channel, min(last)) <- cursor(_, last).
finalize(chan(ordinal, _))    <+ watermark(_, low), chan(ordinal, _), ordinal =< low.
```

rx lowering:

```js
const chan$ = publish$.pipe(scan(appendWithOrdinal, []), shareReplay({ refCount: false }));
const watermark$ = combineLatest(cursors).pipe(map(all => Math.min(...all)));
const retained$ = combineLatest([chan$, watermark$]).pipe(
  map(([rows, low]) => rows.filter(row => row.ordinal > low)),
  distinctUntilChanged(sameRows));
```

Worth stating plainly: **rxjs has the same gap.** `shareReplay` takes a
static `bufferSize` or an unbounded buffer, and nothing in rxjs prunes a
replay buffer against a consumer cursor either. This is not a place where the
language is behind its lowering target.

Two things this proposal does not settle and should not: whether `keep(rule)`
is the right decl word for "this log's retention is a rule", and whether a
retracting edge head is spelled as a `finalize` head or some other way. Both
are the coordinator's call.

---

## THE COLLAPSE EVENT, made concrete

Event shape, as the model mints it:

```
collapse(Tick, Ref, Key, Writes, NetVisible)
```

Four answers to what the ruling leaves open:

1. **Where.** Exactly one instrumentation site is reachable: the keyed store
   write. Every frontier the ruling names (arrival batching, the occurrence
   loop, the drain carry) funnels through it. Duplicate Set adds mint no
   occurrence at all, Log appends keep every stamp, and level rels are
   recomputed with no intra-tick history. A collapse on a drain tick comes out
   of the same site as one on an arrival tick, graded.
2. **What the count is.** WRITES per key per tick, one event per key. Three
   writes mint `collapse(1, latest/2, [cli], 3, true)`, not two events. A
   reader who wants "lost intermediates" subtracts one. Two rules writing one
   key inside ONE occurrence throw `keyed_conflict` rather than collapsing, so
   the count only ever counts writes from distinct occurrences.
3. **Whether it fires with no delta.** It must. The net-zero pair (`v0` to
   `v1` to `v0`) leaves the boundary showing nothing at all for that rel, and
   that is the case where silence is most misleading:
   `collapse(1, latest/2, [cli], 2, false)`. The `NetVisible` flag is what
   distinguishes it. An equal-row rewrite counts too, because the occurrence
   happened even though the store did not move.
4. **Whether it is gradeable.** Not where the ruling puts it.
   `SLOT-COLLAPSE-CHANNEL`: two runs with different collapse counts produce
   byte-identical tick logs for the collapsed rel, so item-9 grading cannot
   see the difference and two runners could disagree about collapses while
   both grade PASS. Either the grading harness diffs a second collapse log
   alongside the tick log, or the event moves into the tick log as a
   distinguished line.

The graded scenario, with the log line:

```
program:  latest(key, value) <+ poll(key, value).       % key(key)
schedule: [[ +poll(cli, v1), +poll(cli, v2) ]]
tick log: 1: [-latest(cli,v0), +latest(cli,v2), +poll(cli,v1), +poll(cli,v2)]
collapse: collapse(1, latest/2, [cli], 2, true)
```

`v1` never existed as far as the program can tell. The collapse line is the
only record that it was ever written.

---

## THE LEVEL-AS-SIGNED-EDGE DEMONSTRATION

```
% level
out(item) <- src(item).

% the signed-edge claim
out(item) <+ src(item).                    % the plus arm
???       <+ finalize(src(item)).          % the minus arm has no head form
```

Plus half, both forms, `+src(a)` at tick 1:

```
level: 1: [+out(a),+src(a)]
edge:  1: [+out(a),+src(a)]
       2: []
```

Every delta agrees. The edge form mints one trailing quiescence tick from the
edge write carrying itself; a level rel feeding an edge rule mints the same
tick, which locates it in the write rather than in the head kind.

Minus half, `-src(a)` at tick 2:

```
level: 2: [-out(a),-src(a)]        final: []
edge:  2: [-src(a)]                final: [out(a)]
edge with finalize arm:
       2: [-src(a)]
       3: [+gone(a)]               final: [gone(a), out(a)]
```

The desugar is inexpressible on the minus half. Edge heads only append or
replace; a `finalize` arm observes the departure but writes a NEW row, one
drain tick later. rx CAN express the desugar, because a `scan` owns a set it
may shrink:

```js
merge(srcAdds$.pipe(map(row => ({ sign: 1, row }))),
      srcRemoves$.pipe(map(row => ({ sign: -1, row }))))
  .pipe(scan(applySignedDelta, new Set()))
```

The gap is in the kernel, not in the idea.

---

## ROUND JOURNAL

Seven rounds. Rounds 1 to 3 live inside the thread files because their
scenarios ARE the thread content; rounds 4 to 7 are pure adversarial passes.
Each of rounds 4, 5 and 6 found exactly one break. Round 7 found nothing and
the fixpoint closed.

### Round 1: build

Aim: build the arm table, both consumption spellings, the channel and the
collapse model; assert what each thread looks like when it works.

- the whole consumption axis turned out to be a decl choice, so the
  switch-versus-queue question is not a construct question at all
- the queue ordinal minted in-language on the first try through `pre/1`,
  which killed a suspected need to expose the engine stamp as a column
- `complete` and `unsubscribe` both grounded to `finalize` on a DIFFERENT
  rel, which is why the arm table needed a subject column the ruled
  vocabulary does not have
- the error arm ran as a plain variant destructure with no engine change
- the collapse event needed a model, because `engine.pl` throws away the
  per-key write counts that produce its boundary diff

### Round 2: change one thing at a time

Aim: break round 1 by changing the consumer key, a column order, the order of
an error row, the retention bound.

- pacing (a) into a keyed consumer lost two of three items, which broke the
  round-1 reading that the pacings differ only in tick index **(amends 4)**
- moving the payload column ahead of the ordinal changed which item survived
  pacing (a), so pacing (a) preserves no stated order at all
- the error arm fired and the rel kept producing the next tick, so the rx
  word promises a termination the language does not deliver
- `keep(count(2))` produced a tick log byte-identical to `keep(all)` while
  permanently stalling a reader
- the net-zero write pair broke the round-1 reading that a collapse event
  annotates a delta: there is no delta to annotate

### Round 3: durability, scale, gradeability

Aim: attack what round 2 left standing, on axes other than data.

- restart from the durable queue produced zero ticks, so the C7-sidestep
  claim is conditional on a boot occurrence policy that does not exist
- re-delivering the durable rows as arrivals also produced nothing, because
  an already-present Set row is not an occurrence
- pacing (b) turned the drain cap into a queue-length cap at exactly 100 items
- the arm family split three-and-three on firing tick, which the ruled
  vocabulary does not say anywhere
- two runs with different collapse counts produced identical tick logs, so
  the ruled trace event is not conformance-checkable where the ruling puts it

### Round 4: edges of the round-1 shapes

Aim: an empty queue, a fully drained queue pushed again, a partially drained
queue on restart, a reader that republishes onto its own channel, a collapse
on a drain tick.

- **FINDING**: the level and edge forms do NOT agree byte for byte. The edge
  write carries itself into a trailing quiescence tick the level form never
  mints **(amends 24, mints 28)**
- the empty queue, the redrained queue and the self-feeding channel all held;
  the `min` aggregate over an empty undrained set produces no head row rather
  than an error or a spurious row
- a partially drained queue stalls on restart exactly like a full one, so
  assertion 6 is not an artefact of the all-or-nothing case
- the drain-tick collapse came out of the same instrumentation site as the
  arrival-tick collapse, confirming assertion 20 at the one place it could
  have needed a second site

### Round 5: duplicates, conflicts, a bound smaller than one batch

- **FINDING**: a Log row appended and pruned inside ONE tick has no delta of
  any sign anywhere in the run **(amends 17, mints 26)**
- the identical program under `keep(all)` shows the row, so the erasure is
  retention and not a Log-rel property
- two identical Log arrivals in one tick get distinct ordinals, so the queue
  does not deduplicate, which is the behaviour a queue must have
- two rules writing one key inside one occurrence throw `keyed_conflict`
  rather than collapsing, so the collapse count only counts writes from
  distinct occurrences

### Round 6: the error arm on a keyed envelope

- **FINDING**: a keyed envelope SWALLOWS an error row that arrives in the
  same tick as a later ok row; the arm never fires and no rel records that a
  failure happened **(amends 12 and 13, mints 27)**
- the same two rows one tick apart do fire the error arm, so whether a
  failure is observed is a function of scheduler batching
- the ruled collapse event reports the drop, the first place in this lab
  where the trace obligation is the only thing between a dropped failure and
  silence
- a Log envelope never swallows the error, so the escape is a decl choice and
  assertion 1 holds again at the error arm

### Round 7: close

Aim: attack the collapse count from the quiet side, stack retention against
the `finalize` refusal on one row, check the pacing (b) drain against the
collapse log, replay every amendment.

**Nothing found.** One write per key per tick mints no event; `finalize` over
a Log rel stays dead even when retention removes the row; the pacing (b)
drain never collapses at its keyed consumer; all five amendments are
journalled. The fixpoint closes.

---

## PROSPECTIVE FIXTURES

Three `fixture/5` terms live in `v6/prolog/labs/consumption_arms/fixtures.pl`
and are graded there by the REAL conformance harness
(`engine:fixture_expectations_hold/2`). They are **not** in
`v6/prolog/conformance/fixtures/**`; promoting them is the coordinator's call.

1. **`lifecycle_arms_on_demand_and_scope_rels`**. `subscribe`, `unsubscribe`
   and `complete` written in shipped kernel words, so the fixture grades the
   claim that the three arms need no construct. 5 ticks; the plus-side arm
   fires in the arrival tick and both minus-side arms one drain tick after
   their minus delta.
2. **`queue_min_ordinal_drains_one_item_per_tick`**. Three requests in one
   tick draining one per tick, with the ordinal minted through `pre/1`.
   `deltas(out/1, [[], [+out(a)], [+out(b)], [+out(c)], []])`, 5 ticks.
3. **`channel_two_readers_one_log_and_a_watermark`**. Two writers in one
   tick, a third in the next, two readers sharing one log, the second waking
   at tick 4 and catching up from its own cursor. The watermark climbs 0, 1,
   2, 3 as the slowest reader does. 7 ticks.

All three passed on their first run against hand-computed expectations.

---

## OPEN SLOTS

| slot | question | state |
|---|---|---|
| `SLOT-ARM-ARGUMENT` | `subscribe`, `unsubscribe` and `complete` read as if they take the data rel and fire on a different one. Refuse the one-argument form, or allow it only where the block statically determines the scope or demand rel | OPEN |
| `SLOT-ERROR-VARIANT-NAME` | reserve `error` as a variant name, or drop the arm word and write the variant arm by its own name | OPEN |
| `SLOT-ERROR-TERMINALITY` | keep the word and write down the difference from rx loudly, or have the arm retract the demand row and become the only arm with a side effect | OPEN |
| `SLOT-RETENTION-SPELLING` | four options priced; s1 (retention as an ordinary rule) named as the smallest honest one, on the argument that retention already deletes log rows invisibly | PROPOSED, not decided |
| `SLOT-COLLAPSE-CHANNEL` | the ruled collapse event is not conformance-checkable on the tracing spine. Diff a second collapse log, or move the event into the tick log | OPEN |
| `SLOT-QUEUE-PACING` | both pacings are expressible with zero new constructs, so the choice is not an expressiveness question. (a) reintroduces the collapse the queue existed to avoid and picks a survivor by term order; (b) preserves per-item observability and hard-fails at 100 queued items | PROPOSED (b), with the drain-cap cost named |
| `SLOT-BOOT-OCCURRENCE` | a durable queue does not resume after a crash because boot seeds `PrevLevel` from the boot level closure. Seeding it empty would resume every queue and re-fire every boot-true level row, which under content-addressed salts is a cache lookup rather than duplicate work, and which collides with the stated endurance goal of no boot replay of unanswered demand | OPEN, new |
| drain cap as queue-length cap | pacing (b) hard-fails at 100 queued items under the error-at-cap ruling. Per-rel drain allowance, a cap counted in work, or a documented maximum depth | OPEN, belongs to the scheduler arc |
