# switch_flow: can switchMap itself flow, what is complete, and how small can the kernel be?

Lab: `v6/prolog/labs/switch_flow.pl`, 1903 lines. **89 checks, all PASS, 70ms.**
Run: `swipl -q -l v6/prolog/labs/switch_flow.pl -g go -g halt`; `-g report` prints
every scenario's per-tick delta stream and final forest.

Contract: `plans/2026-07-27-switch-flow-lab-header.md` (Q1..Q5) plus the
coordinator's anti-bias addendum (Q6 minimize the kernel; the stale-fill ruling
downgraded to provisional and both readings graded).
Base reused verbatim, not reinvented: the forest rows, teardown and tick shape
from `plans/2026-07-27-sub-forest.md` (`git show 2fff3f61:v6/prolog/labs/sub_lifetimes.pl`),
the `scope_min`/`join_max`/`dnf_lifetime` operators from
`plans/2026-07-27-mode-lattice.md` (`git show 2fff3f61:v6/prolog/labs/mode_lab.pl`).
Rulings honored throughout: q1 stamps, q4 next_tick, q6 explicit_marker, q10
retention, r4 departure, r6 evolving pre, r7 boundary diff, storage_integer_keys.

## HEADLINE

**The minimal stored set is zero engine rels and zero new tick phases.** Sections
2 through 6 answer Q1 to Q5 inside the sub-forest's stored model and find the
switch is fully data-driven there. Section 7 then removes the model: a
subscription is an ordinary keyed Set row of the program's own rel, switchMap is
keyed replace on that row, teardown is the ordinary retraction cascade of its
support cone, nesting needs no path, the four flattening policies are the KEY
DECLARATION plus at most one guard, and self-completion is an ordinary edge
write. `sub`, `sub_path`, `demand`, `scope_queue`, `switch_scope`, `scope_done`,
the injected `demanded/2` rule, the `subscribe`/`unsubscribe`/`complete` tick
items and the completion-settling phase are all eliminated, each with a green
scenario. What survives is ONE engine behavior, the demand projection and the
bind lifecycle it drives, plus a conditional second item that exists only if the
stale-fill ruling lands on `drop`.

sub_lifetimes' own finding, "demand is refcounted by IVM support, not by the sub
row", was the whole answer. It just was not pushed to the end.

## 1. The forest model, and the two things it needed

Sections 2 to 6 run inside sub_lifetimes' stored forest, because that is what the
header asked to be graded. Two changes were forced there, both by a check.

### Change A: the switch fires on the OCCURRENCE STREAM, not the arrival list

sub_lifetimes matched `switch_scope` patterns inside phase 0 (`apply_items`), so
the switch could only see rows the outside sent. A switch keyed by a state
register could therefore never fire at all: a register is written by an edge rule
and is never an outside arrival. `the_register_is_never_an_outside_arrival` grades
that the whole schedule of the register scenario contains zero
`+current_route(...)` items; `switch_on_a_state_register_fires` grades that the
scope is planted anyway.

The fix is to match the switch on the same alphabet edge rules already see:
carry-in first, then outside arrivals, then newly-true level rows, one occurrence
at a time. Three consequences, all graded:

| consequence | check |
|---|---|
| a register write reaches the switch at T+1, never same-tick (q4) | `register_switch_lands_one_tick_after_the_write` |
| a same-tick state flap nets to ZERO scope churn, for free | `same_tick_state_flap_nets_to_zero_scope_churn` |
| a fill in the same tick as its own switch is refused (phase 0 runs first) | `a_fill_in_the_same_tick_as_its_switch_is_refused` |

The second is not an extra rule. The carry set is already filtered to
boundary-observable writes (`memberchk(+Row, Deltas)`, the R2 rider), so an
intermediate register state never reaches the switch. Netting is inherited.

### Change B: `switch_scope` grows a fourth column, the policy

`switch_scope(Pattern, ParentScope, Target, Policy)` with
`Policy = switch | exhaust | merge | concat`, one column on an existing
declaration. Section 7 shows the column is itself eliminable.

## 2. Q1 -- can the switch itself flow?

**Yes, completely.** Four levels of "the switch is data", in increasing strength.

**(a) The pattern may be a state register.** `switch_scope(current_route(SessionId,
RouteId), 1, route_data(SessionId, RouteId), switch)` over a keyed Set rel written
only by `current_route(S, R) <+ only(route_change(S, R))`. Receipt:

```
2  +current_route(session_one,settings)     (keyed replace, no forest movement)
3  +sub(1001,1,route_data(session_one,settings)) +sub_path(1001,[1,1001])
   +demand(1002,1001,route_data(session_one,settings),1001)
```

**(b) The pattern may be an enum arm.** `switch_scope(fetch_result(Endpoint,
fresh(Tag, _Body)), 1, body_of(Endpoint, Tag), switch)` is a nested envelope
constructor in trigger position, the enum-match-as-rules shape
`fixtures/state_machine.pl` already uses. The `error` and `unchanged` arms cause
zero churn (`non_matching_arms_cause_zero_scope_churn`); `fresh` swaps
(`matching_arm_swaps_the_scope`).

**(c) The target term may come out of a row.** Rows carry ground compound terms
(`fresh(tag_v1, body_one)` is already a column value in the conformance
fixtures), and a target IS a ground term, so a routing table decides the target
with no program-text change. One decl, three target shapes with different
functors AND arities (`one_switch_decl_serves_three_target_shapes` grades
`[detail_pane/1, feed/1, feed/2]`):

```
routing(fast, feed(fast_lane))  routing(slow, feed(slow_lane, wide_window))
routing(detail, detail_pane(item_a))
session_target(SessionId, TargetTerm) <+ only(open_session(SessionId, RouteId)),
                                        routing(RouteId, TargetTerm).
switch_scope(session_target(_SessionId, TargetTerm), 1, TargetTerm, merge)
```

**(d) The whole switch may come out of one row.**

```
switch_scope(switch_to(ParentScope, TargetTerm), ParentScope, TargetTerm, switch)
```

`universal_switch_decl_carries_no_program_text` checks the program's rule list is
literally `[]` and that the decl's parent and target variables are the pattern's
variables. `universal_switch_plants_under_the_parent_named_by_the_row` runs it.
So the construct budget for "switch" in the stored model is ONE decl form.

Two guardrails came out of the same section. `switch_pattern_variables_are_not_severed`
fires two switches under two different parents in ONE tick and checks neither
binding leaked: the implementation is `foldl` over the decl list with a per-decl
`copy_term`, never `findall`, for the reason `engine.pl:151` documents.
`switch_under_a_closed_parent_is_silent` grades that a switch naming a scope that
closed earlier in the same tick is silence, not the `subscribe_under_dead_scope`
throw an explicit `subscribe` item still gets.

## 3. Q2 -- what is complete?

**Who derives `scope_done`: all three candidates, and they are one mechanism.**
`all_three_completion_sources_are_plain_level_rules` grades that each is a
`(scope_done(_) <- _)` level rule with no new syntax:

| source | rule | check |
|---|---|---|
| terminal enum arm (`Stream(Item, End)`'s End) | `scope_done(Sub) <- sub(Sub,_,Target), stream_row(Target, done)` | `terminal_enum_arm_derives_scope_done` |
| conjunctive body (forkJoin's last input) | `scope_done(Sub) <- sub(Sub,_,_), result_a(_), result_b(_)` | `conjunctive_body_derives_scope_done` |
| explicit rule head | `scope_done(Sub) <- sub(Sub,_,_), close_request(_)` | `explicit_rule_head_derives_scope_done` |

All three tear down on tick 3, retract the scoped view in the same tick
(`completion_retracts_the_scoped_view_in_the_same_tick`) and leave the data behind
(`completed_scope_leaves_its_data_behind`).

**The mode lattice IS the completion calculus at runtime.** A two-level pipeline:
outer scope 1 completes when BOTH end signals fire (`join_max`, conjunction),
inner scope 2 sits under it with its own signal (`scope_min`, disjunction). The
check does not hardcode a formula; it BUILDS one with the mode_lab operators and
evaluates it against the ticks the signals actually fired on:

```prolog
join_max(until([[end_a]]), until([[end_b]]), Outer)  = until([[end_a, end_b]])
scope_min(until([[end_c]]), Outer, Inner)            = until([[end_a, end_b], [end_c]])
formula_first_true(Formula, SignalTicks, Tick)   % max within a clause, min across clauses
```

`formula_first_true/3` reads the DNF the way the runtime behaves: a clause is
satisfied when its LAST signal fires (max), the formula by its EARLIEST satisfied
clause (min). The predicted tick is compared against the tick the forest actually
lost the `sub` row. `completion_composes_by_join_max_at_runtime` quantifies over
9 assignments of (TickA, TickB); `nesting_composes_by_scope_min_at_runtime` over
12 assignments of (TickA, TickB, TickC). Observed deaths range over 3,4,5,6 and
match every time:

| end_a | end_b | end_c | predicted inner death | observed |
|---|---|---|---|---|
| 2 | 4 | 3 | 3 | 3 |
| 2 | 4 | 5 | 4 | 4 |
| 2 | 6 | 5 | 5 | 5 |
| 2 | 6 | 99 | 6 | 6 |
| 3 | 4 | 99 | 4 | 4 |

mode_lab's static claim now has a per-tick runtime witness, not just the two-run
dominance witness sub_lifetimes provided. `join_max` is what a rule body does to
completion, `scope_min` is what nesting does, and the number the formula produces
is a tick index the interpreter agrees with.
`completion_cascade_settles_inside_one_tick` grades `-sub(1,0,outer)`,
`-sub(2,1,inner_c)` and `-stage_view(inner_c,value_one)` in the SAME delta list.

## 4. Q3 -- the full rx contract as tick items

| rx notion | this model | new construct |
|---|---|---|
| `next` | `fill(Target, Salt, Row)`, demand-gated | none |
| `error` | a `fill` whose VALUE is the error arm | none (error-arm-is-a-value) |
| `complete` | a `fill` of the terminal arm plus a `scope_done` level rule | none |
| `subscribe` | `subscribe(SubId, ParentId, Target, Salt)` item | none |
| `unsubscribe` | `unsubscribe(SubId)` item | none |
| `finalize` | a departure rule (`departed/1`, ruling r4) | none |

`error_is_a_value_row_and_never_an_item` grades that every item in the errored
run's schedule is a `fill`, a `subscribe` or an `unsubscribe`: there is no error
item and no error channel.

### The asymmetry at departure, graded three ways

Three runs of ONE program differing only in the terminal item.

**The scoped view cannot tell them apart, and should not.**
`departure_cannot_distinguish_error_complete_teardown` grades that the `live_row`
delta streams of the completed, errored and torn-down runs are identical, tick 3
being `[-live_row(feed_one, value_one)]` in all three. A row leaving is a row
leaving.

**The scope's own death IS observable today, with zero engine change.** `sub/3` is
an ordinary Set rel, so `departed(sub(SubId, _, Target))` binds under r4 with no
new machinery (`scope_death_is_an_ordinary_set_row_departure`). This closes
sub-forest ambiguity 4(a) in the affirmative: no declaration is needed.

**But scope death alone does not distinguish either**, because all three delete the
sub row. The distinction lives in the DATA the scope left behind:

```prolog
ended(SubId, complete) <+ only(departed(sub(SubId, _, Target))), pre(source_row(Target, done)).
ended(SubId, error)    <+ only(departed(sub(SubId, _, Target))), pre(source_row(Target, error(_))).
ended(SubId, teardown) <+ only(departed(sub(SubId, _, Target))),
                          not(source_row(Target, done)), not(source_row(Target, error(_))).
```

`the_three_way_ending_is_derivable_by_joining_the_data` grades
`[ended(1, complete)]`, `[ended(1, error)]`, `[ended(1, teardown)]` from the three
runs. Receipt for the errored run:

```
3  -live_row(feed_one,value_one) -sub(1,0,feed_one) -demand(...) +source_row(feed_one,error(500))
4  +closed_row(feed_one,value_one) +ended(1,error)
```

So the engine never needs a reason column. Error and complete leave a value row;
teardown leaves none, and "leaves none" is a negated join. This is strictly more
than rxjs `finalize` gives, which cannot tell you why it ran either.

## 5. Q4 -- flattening strategies as ONE policy parameter

**Three of the four are one parameter and no state.**
`the_four_policies_differ_only_in_the_policy_word` grades that the four programs
are variant-identical apart from the policy word and that their forests differ.
After both tabs open (tick 3):

| policy | live scopes | mechanism |
|---|---|---|
| `switch` | `panel, tab(tab_b)` | prefix-DELETE the children, then plant |
| `exhaust` | `panel, tab(tab_a)` | if any child lives, discard the value |
| `merge` | `panel, tab(tab_a), tab(tab_b)` | plant, never tear down |
| `concat` | `panel, tab(tab_a)` + one `scope_queue` row | if a child lives, enqueue |

`exhaust_policy_drops_the_ignored_value_permanently` grades the honest
consequence: the ignored tab is never opened, ever.

### concat: the kernel version, stated exactly

`concat` needs an ORDERED pending set, which the forest does not have:

```
scope_queue(QueueId, ParentId, ParentPath, Target)
```

- **QueueId** is a dense engine integer from a third sequence
  (`concat_queue_ids_are_dense_integers` grades `[1001, 1002]`,
  storage_integer_keys). The id IS the arrival order, so "oldest first" is one
  `msort`, no timestamp column.
- **ParentPath** is the parent scope's materialized path, so teardown stays a
  path-prefix range DELETE. **Teardown goes from 3 range DELETEs to 4**, still
  independent of subtree size (`concat_queue_rows_die_with_the_parent_scope`).
- **The drain runs in the completion-settling phase**, in the same loop that tears
  down finished scopes. `concat_queue_drains_on_completion_in_the_same_tick`
  grades `-sub(1001,...)`, `+sub(1002,...)` and `-scope_queue(1001,...)` in ONE
  delta list; `concat_queue_serves_in_arrival_order` grades tab_b then tab_c.

### concat without kernel state, in the forest model

`concat_is_reproducible_without_kernel_state` runs concat at queue depth one with
`exhaust` policy plus four ordinary rules over one keyed register, using
`departed(sub(_, 1, tab(_)))` as the dequeue trigger. The price is measured:
`userland_concat_costs_two_ticks_the_kernel_queue_costs_zero` grades kernel death
tick 4 and plant tick 4 against userland death tick 4 and plant tick 6. Section 7
shows the derived model gets this to ONE tick with no kernel queue at all.

## 6. Q5 -- switch x state machine

**takeUntil is keyed replace plus a negated `scope_done`.** Two lines on top of
`fixtures/state_machine.pl`, unchanged otherwise:

```prolog
switch_scope(phase(Endpoint, fetching), 1, fetch_of(Endpoint), switch)
scope_done(SubId) <- sub(SubId, _, fetch_of(Endpoint)), not(phase(Endpoint, fetching)).
```

The scope lives exactly while the register holds the state (born tick 2, dead
tick 3), and leaving the state kills it in the SAME tick the register moves
(`leaving_the_state_tears_the_scope_down_in_the_same_tick` grades
`-sub(1001,...)`, `-phase(gh_repos,fetching)` and `+phase(gh_repos,idle)` in one
delta list). The in-flight fetch fill addressed to the dead scope is refused by
the same gate that makes a timer lazy.

### The state flap nets to ZERO scope churn

The header's question, answered: **zero, not two teardowns.** Same tick, an error
arm and a fresh `poll_due`, so the register goes fetching to idle to fetching
inside one tick:

```
3  -retries(gh_repos,0) +retries(gh_repos,1) +fetch_result(gh_repos,error(500)) +poll_due(gh_repos)
```

No `sub` delta, no `phase` delta, no `fetch_wanted` delta, and the retry still
counts. The reason is structural: the switch reads the carry, the carry is
filtered to boundary-observable writes, and the R2 rider already says intermediate
fold states are not observable. The same two events on SEPARATE ticks cost exactly
one teardown and one fresh plant, births at ticks 2 and 6, death at tick 3.

### One parent is one slot (not in the header)

The state register keys the TARGET. The flattening slot is keyed by the PARENT.
With two endpoints under one parent and `switch` policy the two keys fight over
one slot, and the loser is planted and torn down inside a single tick so the
boundary never sees it (`one_parent_scope_is_one_flattening_slot` grades
`scope_birth_ticks(fetch_of(gh_repos)) == []`, an invisible scope). Under `merge`
each key gets its own sibling scope and the negated `scope_done` ends them
independently. So `switch` on a keyed register is almost certainly a program bug
whenever the register has more than one key.

## 7. Q6 -- MINIMIZE THE KERNEL

Adversarial pass on the 7-item absorption list. For each stored rel: is it stored
because it must be, or because the first design stored it?

### 7.1 `sub` and the switch declaration: eliminated by keyed replace

The scope root row is an ordinary keyed Set row of the program's own rel. A new
outer value REPLACES it. Keyed replace retracts the old row, IVM retracts
everything the old row supported, and that is switchMap:

```prolog
keyed(open_scope/3, [1]),  keyed(scope_instance/2, [1])

scope_instance(SessionId, Next) <+ only(route_change(SessionId, _)),
                                   pre(scope_instance(SessionId, SoFar)), Next := SoFar + 1.
open_scope(SessionId, Next, route_data(RouteId)) <+ only(route_change(SessionId, RouteId)),
                                   pre(scope_instance(SessionId, SoFar)), Next := SoFar + 1.
demanded(Target, Instance) <- open_scope(_, Instance, Target).
route_view(RouteId, Body)  <- demanded(route_data(RouteId), _), route_row(RouteId, Body).
```

Receipt (`derived_switch`, the whole program; note tick 3 does all of switchMap
with no teardown statement in existence):

```
1  -scope_instance(session_one,0) +scope_instance(session_one,1)
   +open_scope(session_one,1,route_data(settings)) +demanded(route_data(settings),1)
2  +route_row(settings,body_settings) +route_view(settings,body_settings)
3  -open_scope(session_one,1,route_data(settings)) -demanded(route_data(settings),1)
   -route_view(settings,body_settings)
   +open_scope(session_one,2,route_data(profile)) +demanded(route_data(profile),2)
4  +route_row(profile,body_profile) +route_view(profile,body_profile)
```

Checks: `keyed_replace_alone_is_switch_map`,
`the_derived_scope_retracts_its_demand_with_no_teardown_statement`,
`the_derived_model_stores_no_forest_row` (empty forest across all four derived
scenarios), `the_derived_model_declares_no_engine_construct` (no `switch_scope`
decl, no `scope_done` rule, no `subscribe`/`unsubscribe`/`complete` item in any
schedule), `the_program_owns_its_demand_rule_so_the_engine_injects_none`.

The instance column is the program's own counter, the same `pre` + `:=` scan the
retries fold in `fixtures/state_machine.pl` uses
(`the_instance_column_is_ordinary_program_state`), and it is what keeps the
stale fill refusable (`the_derived_gate_still_refuses_the_stale_fill`).

### 7.2 `sub_path`: eliminated with the teardown statement

The materialized path existed only to make teardown one range DELETE. With no
teardown statement there is nothing to make cheap. Nesting works because the inner
scope's liveness JOINS the outer's row, so retracting the outer retracts the
inner transitively:

```prolog
live_detail(PaneId, detail(ItemId)) <- open_pane(PaneId, _), open_detail(PaneId, ItemId).
demanded(Target, PaneId)            <- live_detail(PaneId, Target).
```

```
3  -open_pane(pane_one,item_list) -live_detail(pane_one,detail(item_a))
   -demanded(detail(item_a),pane_one) -detail_view(item_a,body_a)
```

Checks: `outer_retraction_cascades_to_the_inner_scope`,
`no_path_row_is_needed_for_nested_teardown` (structural AND behavioral),
`data_written_under_a_derived_scope_survives_it`.

### 7.3 The policy parameter: eliminated, it is the KEY DECLARATION

Same three rules in all three programs; only the key differs
(`the_three_derived_policies_share_every_rule` grades the rule lists variant-equal
and the two keys different):

| policy | declaration | result after two opens |
|---|---|---|
| switch | `keyed(open_tab/2, [1])` | `[live_tab(tab_b)]` |
| merge | `keyed(open_tab/2, [1, 2])` | `[live_tab(tab_a), live_tab(tab_b)]` |
| exhaust | `keyed(open_tab/2, [1])` plus one body guard `not(live_tab(_))` | `[live_tab(tab_a)]` |
| concat | exhaust plus the departure replay | serves in order, one tick per dequeue |

`switch` keys the scope row by the outer identity so a new value replaces the old.
`merge` adds the value to the key so both coexist. `exhaust` keeps the switch key
and adds the guard. This is the same fact stated twice: the flattening strategy is
what the scope row's PRIMARY KEY says about how many can exist at once.

concat in the derived model is strictly better than in the forest model: the
dequeue is ONE tick, not two, because the replay's write IS the scope rather than
a row a swap has to observe next tick
(`the_derived_concat_dequeue_costs_one_tick_not_two`). Receipt:

```
1  +open_tab(session_one,tab_a) +live_tab(tab_a) +demanded(tab(tab_a),tab_a)
2  -pending_tab(session_one,none) +pending_tab(session_one,tab_b)
3  +tab_closed(tab_a) -live_tab(tab_a) -demanded(tab(tab_a),tab_a)
4  -open_tab(session_one,tab_a) +open_tab(session_one,tab_b)
   +live_tab(tab_b) +demanded(tab(tab_b),tab_b) +pending_tab(session_one,none)
```

### 7.4 `scope_done` and the completion-settling phase: eliminated

Self-completion is an EDGE write into a rel strictly upstream of the scope root
row, and the scope's liveness negates it:

```prolog
fork_closed(SessionId)        <+ result_a(_), result_b(_), open_fork(SessionId, _).
live_fork(SessionId, Target)  <- open_fork(SessionId, Target), not(fork_closed(SessionId)).
demanded(Target, SessionId)   <- live_fork(SessionId, Target).
```

The teardown lands in the SAME tick as the last arm, with no settling phase, because
the edge write reaches the store before the post-write level closure runs:

```
3  +result_b(beta) +fork_closed(session_one)
   -live_fork(session_one,arm_target) -demanded(arm_target,session_one) -fork_view(alpha)
```

Checks: `self_completion_needs_no_settling_phase`,
`self_completion_leaves_the_arms_and_drops_the_late_one`,
`self_completion_negation_is_stratified_by_construction`.

The last one is a real restriction, not a formality. The tempting alternative,
deriving the completion condition as a LEVEL rule over rows produced UNDER the
scope, closes a negative cycle through the demand edge
(`live -neg-> done -> result -demand-> live`) and is unstratifiable. The edge
write breaks the cycle because the Log row, once written, is a fact rather than a
re-derived conclusion. **This is the law that replaces the settling phase**, and
it is a check the language owes, not a runtime feature.

### 7.5 The tick alphabet: `subscribe`/`unsubscribe`/`complete` eliminated

A subscription starting is `+open_pane(...)`, a subscription ending is
`-open_pane(...)` or a `closed` row, and completing is section 7.4. All three are
ordinary arrivals into ordinary rels, so the outside talks to the engine through
one channel instead of two. `the_derived_model_declares_no_engine_construct`
grades that no derived scenario's schedule contains any of the three items.

### 7.6 What actually survives

| survivor | why it cannot be derived | where it lives |
|---|---|---|
| **the demand projection + bind lifecycle** | an effect must start when something demands it and stop when nothing does; this is magic-set demand and it is the whole point | one engine BEHAVIOR, not a rel; the projection is an ordinary derived rel IVM already materializes |
| **the scope root row** | nothing derives "the user opened this pane" | an ordinary PROGRAM keyed Set rel |
| **the concat pending set** | values that already occurred and are waiting have no derivation anywhere in the store | an ordinary PROGRAM keyed rel (`pending_tab/2`), 4 rules at depth one |
| **per-instance demand identity** | only if the stale-fill ruling lands on `drop`; see section 8 | an ordinary PROGRAM column plus a `pre` + `:=` counter, or an engine sequence |
| **stratification of self-completion** | not storage; a static law (7.4) | a check the language owes |

Nothing on that list is an engine table.

### 7.7 What the minimal kernel COSTS

Honest accounting. The kernel loses 3 rels, 1 injected rule, 1 declaration, 1
completion signal, 3 tick item kinds and 1 tick phase. The PROGRAM gains, per
switch site:

| forest model | minimal kernel |
|---|---|
| `switch_scope(select(ItemId), 1, detail(ItemId), switch)` | `keyed(open_scope/2, [1])` decl |
| | `open_scope(PaneId, detail(ItemId)) <+ only(select(PaneId, ItemId)).` |
| | `demanded(Target, PaneId) <- open_scope(PaneId, Target).` |
| `scope_done(Sub) <- ...` when self-completing | `live_scope(...) <- open_scope(...), not(closed(...)).` plus the edge rule that writes `closed` |
| engine-minted salt | an instance column and a 2-line counter, IF drop is ruled |

So roughly 1 line becomes 3, or 5 with instance identity. Two claims this lab
CANNOT settle, both flagged as ambiguities: whether the retraction cascade costs
the same as the range DELETE it replaces (a Prolog interpreter cannot measure
that), and whether a surface sugar should generate the 3 to 5 lines so authors
still write one.

## 8. The stale-fill trichotomy (ruling downgraded to provisional)

Three readings, costed in primitives rather than argued.

| reading | behavior | primitives it needs |
|---|---|---|
| **abort-on-teardown** | demand deletion IS the cancel; no orphan fill exists | ZERO. The demand projection already exists; cancellation is a bind-layer property. |
| **orphan-as-a-row** | the response lands in an ordinary rel; the VIEW is what is scoped, so no reader sees it until one demands it | ZERO. And the fill GATE becomes unnecessary, which removes the last tick item too. |
| **drop** | the gate refuses the fill | ONE: per-instance demand identity, which nothing else in the model needs. |

**`drop` is the only reading that costs anything, and this is measurable.** With
content-addressed demand (the key IS the content, no instance column), closing a
subscription and reopening an IDENTICAL one makes the first subscription's
response indistinguishable from an answer to the second, and it is admitted:
`content_addressed_demand_cannot_detect_a_stale_fill`. Adding an instance column
restores the refusal: `an_instance_column_restores_stale_detection` grades
`first_response` absent and `second_response` present.

**`abort` and `drop` are observationally identical in the store.**
`abort_on_teardown_and_drop_are_indistinguishable_in_the_store` runs the same
program and schedule twice, once with the orphan fill item and once without, and
grades the two final stores equal. The difference between the two readings is
therefore entirely about whether the effect kept running, which is a resource
question, not a semantics question.

**`orphan-as-a-row` is the reading that pays for itself.**
`an_orphan_admitted_as_a_row_is_reused_by_the_next_subscriber`:

```
1  +open_feed(session_one,alpha) +demanded(feed(alpha),alpha)
2  -open_feed(session_one,alpha) -demanded(feed(alpha),alpha)
3  +cache_row(alpha,orphan_body)                          (nobody subscribed; no view moves)
4  +open_feed(session_two,alpha) +demanded(feed(alpha),alpha)
   +feed_view(alpha,orphan_body)                          (no refetch)
```

That is the SWR case sub-forest ambiguity 1 described, and in the minimal kernel
it needs no cache declaration and no `dead_letter` rel: the row is eternal, the
VIEW is scoped, and the next subscriber's join finds it.

**Which falls out of fewer primitives:** `orphan-as-a-row`, by one. It removes the
instance column AND makes the `fill/3` tick item collapse into an ordinary `+Row`,
which takes the minimal kernel's tick-alphabet delta to zero. `abort-on-teardown`
ties on primitives but only holds when the transport can actually cancel, and it
degrades into `orphan-as-a-row` the first time one cannot. `drop` is the only one
that adds a primitive, and it is the reading sub_lifetimes graded. Lab position,
stated as a position and not a ruling: **admit the row, scope the view.**

Caveat this lab cannot close: `orphan-as-a-row` means a response with no reader
still consumes storage, bounded only by the rel's `keep` clause. For a cache
that is the point; for a firehose it is a leak. mode_lab ambiguity 1 already
holds that `keep` and lifetime are separate axes, and this is the case where the
separation gets tested.

## 9. ENGINE-ABSORPTION DELTA vs the 7-item sub-forest list

`plans/2026-07-27-sub-forest.md`, section "What the conformance engine must GAIN".
Two columns because two models were graded: the forest model the header asked for,
and the minimal kernel the addendum asked for.

| item | forest model after this lab | MINIMAL KERNEL |
|---|---|---|
| 1. three store rels (`sub`, `sub_path`, `demand`) | **ADD conditional 4th**: `scope_queue(QueueId, ParentId, ParentPath, Target)` if `concat` is a kernel policy | **DELETE ALL FOUR.** The scope root is a program keyed Set rel; the path is unnecessary once teardown is retraction; the demand rows are a derived projection. Graded: `the_derived_model_stores_no_forest_row` |
| 2. injected level rule `demanded/2` | unchanged | **DELETE.** The program heads `demanded/2` itself; the engine injects only when it does not. Graded: `the_program_owns_its_demand_rule_so_the_engine_injects_none` |
| 3. `switch_scope(Pattern, ParentScope, Target)` | **MODIFIED**: fourth `Policy` column; all arguments may be shared variables over one row, so the universal spelling subsumes every program-specific decl | **DELETE.** switchMap is keyed replace; the policy is the key declaration plus at most one guard. Graded: `keyed_replace_alone_is_switch_map`, `switch_is_the_scope_row_keyed_by_the_outer_identity`, `merge_is_the_same_rules_with_the_value_added_to_the_key`, `exhaust_is_the_switch_key_plus_one_guard`, `concat_is_exhaust_plus_the_departure_replay` |
| 4. `scope_done(SubId)` | unchanged; three derivation sources graded, all ordinary level rules | **DELETE.** Completion is an edge write into a rel strictly upstream plus a negation. Graded: `self_completion_needs_no_settling_phase`. **REPLACED BY A LAW**: the completion condition may not be a level rule over rows produced under the scope (unstratifiable) |
| 5. tick-item alphabet | unchanged; `next`/`error`/`complete`/`finalize` all fit inside it | **REDUCE to `+Row \| -Row \| fill(Target, DemandKey, Row)`**; `subscribe`/`unsubscribe`/`complete` become ordinary arrivals. Under the `orphan-as-a-row` reading `fill` collapses into `+Row` and the delta is **zero** |
| 6. two tick-phase changes | **MODIFIED**: phase 0 no longer applies swaps (they move into the occurrence pass); the settling phase gains the concat drain | **DELETE the settling phase.** Cascades are level closure, which already loops. Phase 0 stays ordered only for the fill gate, and only under the `drop` reading |
| 7. diag row for a refused fill | **WIDEN**: a swap under a dead parent and an `exhaust`-discarded value are also silent drops | **SURVIVES only under `drop`.** Under the other two readings nothing is refused, so there is nothing to diagnose |

**MINIMAL STORED SET: zero engine rels, zero engine tick phases, zero engine
declarations.** One engine BEHAVIOR survives, the demand projection and the bind
lifecycle it drives. One tick item survives conditionally, `fill` with a demand
key, and only if the stale-fill ruling lands on `drop`.

Standing facts this lab establishes regardless of which model is chosen:

- `departed(sub(...))` and `departed(live_tab(...))` both already work, closing
  sub-forest ambiguity 4(a): scope death is an ordinary Set/level departure.
- `scope_min`/`join_max` belong in the shared module mode_lab.md asked for, and
  `formula_first_true/3` belongs beside them: it is the teardown planner's
  arithmetic and the CLI's "this will finish at" answer.
- The flattening strategy and the scope row's primary key are the same fact.

## 10. AMBIGUITIES (numbered; ruling-needed flags)

1. **Forest model or minimal kernel?** `[RULING NEEDED, blocks everything else
   here]` Both are green. The forest costs 3 to 4 engine rels, 1 declaration, 1
   signal, 3 item kinds and 1 tick phase, and buys 1-line switch sites and a
   teardown whose cost is provably independent of subtree size. The minimal
   kernel costs 3 to 5 program lines per switch site and buys an engine with no
   subscription concept in it at all. **Blocks:** whether ambiguities 2, 3, 5, 7
   and 9 below exist at all, since most of them are forest-model questions.

2. **May a row name its own parent scope?** `[RULING NEEDED if the forest model
   is chosen]` The universal decl `switch_scope(switch_to(ParentScope,
   TargetTerm), ParentScope, TargetTerm, _)` is what makes the switch fully data,
   and it is graded working. It also lets a program write an arbitrary integer sub
   id into a row and plant under someone else's subscription, exactly the
   per-subscriber-state-in-shared-rows LANG.md forbids. Options: (a) allow it and
   accept that scope ids are addressable data; (b) lexically bound parent only
   (sub-forest ambiguity 6); (c) row-carried parent only when the row was written
   by a rule inside that scope. **Note:** the minimal kernel does not dodge this.
   The scope root row is an ordinary rel, so any rule can write it and any client
   can retract it. The hazard moves; it does not go away.

3. **Should the switch fire on departures?** `[no ruling needed]` The lab fires
   swaps on arrival occurrences only. A "restart when the row goes away" switch is
   expressible today by routing the departure through an edge rule into a rel the
   switch does match, which is exactly what both concat implementations do.

4. **Which stale-fill reading?** `[RULING NEEDED, the addendum's own question]`
   Section 8 costs all three. `orphan-as-a-row` needs one fewer primitive than
   `drop` and additionally collapses the `fill` tick item; `abort-on-teardown`
   ties on primitives but only when the transport can cancel. **Blocks:** whether
   per-instance demand identity exists, whether the `fill` item exists, whether
   the refused-fill `diag` row exists, and the effect-rel lowering.

5. **A fill in the same tick as its own switch is refused.** `[forest model only]`
   Phase 0 (fills) runs before the occurrence pass (swaps), graded by
   `a_fill_in_the_same_tick_as_its_switch_is_refused`. A real effect cannot round
   trip inside one tick, so this only constrains how canned fixture fills are
   written. Does not exist in the minimal kernel, where the scope row is written
   by an ordinary edge rule and the level closure catches up in the same tick.

6. **`switch` on a multi-key register is probably always a bug.** `[RULING NEEDED,
   static-check owner unassigned]` One parent scope is one flattening slot, so
   `switch_scope(phase(Endpoint, fetching), 1, fetch_of(Endpoint), switch)`
   silently serializes every endpoint and the losing plant is invisible at the
   boundary. In the minimal kernel the same bug is spelled `keyed(open_tab/2,
   [1])` when it should be `[1, 2]`, which is at least a declaration a static
   check can read. Options: (a) reject when the pattern's key columns are not all
   constant; (b) nothing, and document the footgun.

7. **A scope that ends with no rows is indistinguishable from one that never
   started.** `[no ruling needed]` The three-way `ended/2` classification works by
   joining the terminal row the scope left behind. A scope that completes having
   produced nothing has no row to join and classifies as `teardown`. The honest
   fix is a completion rule that writes its own marker row.

8. **Is `concat`'s pending set kernel or program?** `[RULING NEEDED]` It is the
   one thing in this lab with no derivation anywhere, so it is stored either way.
   Kernel: one rel, one extra range DELETE in teardown, same-tick drain. Program:
   zero engine cost, 4 rules at queue depth one, one-tick dequeue in the derived
   model and two-tick in the forest model, and roughly 6 rules with an index for
   arbitrary depth. **Blocks:** whether the word `concat` names anything the
   engine knows. `switch`, `exhaust` and `merge` are free under both models.

9. **Does the retraction cascade cost what the range DELETE cost?** `[UNMEASURED,
   flagged rather than claimed]` The forest's teardown was 3 range DELETEs
   independent of subtree size. The minimal kernel's teardown is a semi-naive IVM
   pass over the support cone. For the scoped VIEW rows the two touch the same
   set, so it should be a wash; for deep scope NESTING the forest deleted the
   whole subtree with one range scan. A Prolog reference interpreter cannot
   measure this, and no claim in section 7 depends on it. It wants a lowering-tier
   benchmark before the minimal kernel is committed to.

10. **Should a surface sugar generate the minimal kernel's 3 to 5 lines?**
    `[no ruling needed to proceed]` If `switch_map` becomes sugar that expands to
    a keyed rel plus two rules, authors keep the one-line spelling and the engine
    keeps zero subscription concepts. That is the best of both, and it is exactly
    the shape ruling `cut_pipe` used for `|>`: defer the sugar, keep the desugared
    rules valid. Nothing blocks on it; it decides only how the surface reads.

11. **Do `scope_queue` rows belong in the public delta stream?** `[forest model
    only, same shape as sub-forest ambiguity 10]` They are rows, so r7 emits them.
    Same answer as the rest of the forest, whatever that turns out to be. In the
    minimal kernel the question is moot: the pending set is program data and
    always was public.
