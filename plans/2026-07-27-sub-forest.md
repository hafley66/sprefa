# sub_lifetimes: the subscription forest (runtime half of lifetimes)

> **SUPERSEDED 2026-07-27 late PM** by rulings.pl `subscription_kernel =
> minimal_with_coverage_check_and_ghost_view`: the stored forest below was
> eliminated by plans/2026-07-27-switch-flow.md section 7 and the red-team pass
> (plans/2026-07-27-redteam-minimal-kernel.md). Kept as the record of the design
> that lost and WHY its mechanism (demand rows, prefix teardown) still shaped the
> winning one.

Lab: `v6/prolog/labs/sub_lifetimes.pl`. 41 checks, all PASS. DELETED per lab
protocol; last lives at commit 2fff3f61, recover via
`git show 2fff3f61:v6/prolog/labs/sub_lifetimes.pl`.
Run (from that checkout): `swipl -q -l sub_lifetimes.pl -g go -g halt`; `-g report` traces.

Thesis under test (user, 2026-07-27): sets and dbs do not have lifetimes, rx
scripts do. Rows are eternal (retention-bounded); lifetime lives only in the
subscription forest; teardown kills the script, never the data.

## Verdict

The thesis holds under the conformance tick with **three new rows and one new
tick phase**. No new time coordinate, no per-subscriber row copies, no
callback registry, no retraction hooks, no lifetime column anywhere.

Everything the task asked for came out of one mechanism, demand rows:

| rx idea | this model |
|---|---|
| subscribe | INSERT a `sub` row, its `sub_path` row, its `demand` rows |
| laziness | a bind fill is admitted only while a `demand` row for its `(Target, Salt)` exists |
| unsubscribe / teardown | range-DELETE every forest row whose path has the scope's path as a prefix |
| scoped view retracts | ordinary IVM: the view joined `demanded/2`, support went away |
| `switchMap` | outer arrival deletes the scope's children, plants a new child with a fresh integer id |
| dominance | the inner timer's demand row was inside the deleted prefix |
| `forkJoin` completion | a `scope_done(SubId)` level row makes the scope delete its own subtree at tick end |
| `repeat` | a new sub id and a fresh salt, so `demanded/2` gains a row and the effect re-fires |
| stale response | the demand row is gone, so the fill is refused by the same gate that makes the timer lazy |

The last row is the strongest finding: **the stale-response problem is not a
special case**. The check that drops an in-flight fill for a dead scope is
byte-identical to the check that keeps an undemanded timer silent.

Second finding: **demand is refcounted by IVM support, not by the sub row**.
`demanded(Target, Salt) <- demand(_, _, Target, Salt)` means a request lives
while any subscriber's demand row supports it. `demand_is_refcounted_by_support`
grades a scope being torn down with the request still alive underneath, because
a second subscriber holds it. That is the count-IVM callout applied to the
forest, and it removes any need for a refcount column.

Third finding, the one that bites program authors: **a conjunctive join written
as a level rule dies with its scope**. `fork_join_level_view_dies_with_the_scope`
and `fork_join_combined_row_outlives_the_scope` are the same join, one as a
level view under the scope and one as an edge write into a Log rel. Only the
second survives self-teardown. `conformance/fixtures/operators.pl` models
`fork_join` as a level rule, which is correct there only because its inputs are
unscoped Set rels.

## What the conformance engine must GAIN

Precise list. Everything not on it stays as it is, and forest rows flow through
the existing machinery as ordinary Set rows.

### 1. Three store rels (engine-written, program-readable)

```
sub(SubId, ParentId, Target)              % the script node; root parent = 0
sub_path(SubId, [Segment, ...])           % materialized path over INT segments
demand(DemandId, SubId, Target, Salt)     % the demand rows a subscribe plants
```

All keys integers (ruling `storage_integer_keys`). Two dense integer
sequences, one per table, engine-owned, never reused after a teardown. The lab
carries them in tick state, deliberately NOT as store rows, so they never
appear in the boundary deltas.

sqlite shape for `sub_path`: either a packed-integer BLOB per row (prefix
teardown is `DELETE ... WHERE path >= :prefix AND path < :prefix_upper`, one
statement per table) or `sub_path(sub_id, depth, segment)` rows plus a join.
Both keep teardown at 3 statements, independent of subtree size. Materialized
path beat adjacency-list recursion here for exactly the reason
`i:graph-libs:relational-graph-patterns:04` gives: prefix delete is a range
scan, child recursion is not.

### 2. One engine-injected level rule

```
demanded(Target, Salt) <- demand(_DemandId, _SubId, Target, Salt);
```

Programs join `demanded/2` to scope a view. `DemandId` and `SubId` stay hidden
from the program on purpose: if a rule could join per-subscriber, subscription
relative state would be back in shared rows, which LANG.md forbids.

### 3. One new declaration (surface `switch_map`)

```
switch_scope(Pattern, ParentScope, Target)
```

On an arrival matching `Pattern`: teardown the children of `ParentScope`
(prefix delete, exclusive of the parent), then subscribe a fresh child on
`Target`. The new sub id IS the salt. The lab writes `ParentScope` as a literal
integer; the real surface has to bind the enclosing scope lexically
(ambiguity 6).

### 4. One new completion signal

A `scope_done(SubId)` level row means the scope has all its terminal rows.
The engine tears that scope's subtree down, itself included. Chosen over a
`complete_when(Scope, Condition)` declaration because forkJoin's condition is
already an ordinary join and needs no new syntax; the cost is that the engine
reads a rel by name, which the v5 magic-rel ban forbids (ambiguity 8).

### 5. New tick-item alphabet (fixture format change)

Today a tick's input is a list of `+Row` / `-Row`. It has to become an ordered
list over:

```
+Row | -Row                            outside arrival (unchanged)
subscribe(SubId, ParentId, Target, Salt)
unsubscribe(SubId) | complete(SubId)
fill(Target, Salt, Row)                a bind fill, demand-gated
```

`fill/3` is the biggest change for existing fixtures: any row a bind fills must
carry the `(Target, Salt)` it answers, or the engine cannot refuse it. Canned
rows stay canned (the bind-at-link law is untouched); they just get addressed.

### 6. Exactly two tick-phase changes

- **Phase 0 becomes ordered over the mixed alphabet.** `absorb_arrivals` grows
  into `apply_items`: forest ops mutate the forest in place, fills are gated on
  demand presence *at their position in the list*, and an arrival can trigger a
  scope swap before the next item is read. Within-tick order therefore decides
  whether the last fill of a dying subscription lands
  (`within_tick_order_decides_the_last_fill` grades both orders).
- **A completion-settling phase between retention and the boundary diff.**
  Recompute the level views, tear down every live `scope_done` scope, repeat to
  fixpoint. It must run before the boundary diff so the scope's death and its
  views' retraction are one tick, and it must loop because one completion can
  make an enclosing scope's `scope_done` true.

Unchanged: level closure, occurrence firing one at a time, `pre` on the
evolving store, keyed replace, r7 boundary deltas, next-tick carry, engine
drains, r4 departures, retention.

### 7. One diagnostic gap

A refused fill is currently silent. The engine should write a `diag` row per
drop (ruling `a6_diag`: `diag` is an ordinary rel, the CLI is a consumer).
Without it, "my effect produced nothing" has no on-disk answer, which is the
same class of defect as the v5 `"15 changed path(s)"` incident.

## Deviations from LANG.md

1. **Demand dedup is split in two.** LANG.md says demand rows are requests with
   content-addressed dedup. Dedup at the row level cannot work, because a
   shared demand row cannot be prefix-deleted by one subscriber without killing
   the other. This lab keeps demand rows per-sub and puts the dedup in the
   `demanded/2` level view, where IVM support does the refcounting. Request
   identity is `(Target, Salt)`; lifetime is per demand row.
2. **`forkJoin`'s combined row is an edge write into a Log rel**, not the level
   rule `conformance/fixtures/operators.pl` uses. Under a scope the level form
   retracts on completion, which contradicts "data outlives script". Both forms
   are graded here; neither fixture is wrong for its own inputs.
3. **The reference interpreter is per-row and per-item on purpose** (ruling
   `n1_statement_budget`: the reference engine is the spec, lowering carries the
   budget). Teardown is the one place where the lowering must be checked against
   the reference: 3 range DELETEs, never one per row.
4. **`switch_scope` names its parent scope by literal id.** The lab is about
   runtime semantics, so lexical scope binding is left to surface_dcg.

## Ambiguities (numbered, for the ruling queue)

1. **Stale fill under a dead scope.** Lab choice, graded three times: DROP,
   silently. Reason: the demand row IS the request; a response with no request
   has no key to attach to, and admitting it would write rows for a script that
   no longer exists. The competing reading is genuinely attractive and I cannot
   settle it from the code: for a CACHE rel (`fetch` answering into
   `cache(endpoint, entry)`) the body is worth keeping even though the pane that
   asked for it closed, because rows are eternal and the next subscriber would
   otherwise refetch. Options: (a) drop (current), (b) admit the row into the db
   but never into the scoped view, (c) admit only for rels declared as caches,
   (d) route to a `dead_letter` rel. Needs a user ruling. It is the difference
   between "the network response is trash" and "the network response is data
   that happens to have no reader yet".
2. **Are `sub` / `demand` rows a Log or a Set rel?** Lab models them as Set: the
   script exists or does not, teardown is a real deletion, and its `-deltas` are
   how the outside sees a subscription end. A Log modeling cannot delete
   anything, so teardown would need a tombstone column, which contradicts
   `technique(teardown, path_prefix_delete)`. An audit trail is still derivable
   as a separate Log rel fed by an edge rule over the forest deltas. Open:
   whether that audit rel is standard.
3. **Who assigns sub ids and salts?** Lab: the engine, one dense sequence per
   table, never reused, and the default salt IS the new sub id. Consequence
   worth ruling on: if the salt is the sub id then two subscribers to the same
   target never share a request, which is the opposite of content-addressed
   dedup. The lab shows both behaviors in one file (switch scopes get fresh
   salts and never share; explicit subscribes with the same salt do share, and
   `resubscribe_with_the_same_salt_is_silent` grades the silence).
4. **Does teardown emit departures?** Lab: yes. A level view dying because its
   scope died is a `-delta` like any other, and `ui_teardown_fires_departure_rules`
   grades a `departed/1` rule firing for all three dying list rows, with the
   consequence landing in an unscoped Log rel that survives. Three open pieces:
   (a) should a scope's own death be bindable (`departed(sub(...))`)? (b) rows
   that die because the DATA changed and rows that die because the SCOPE died
   are currently indistinguishable, which is either elegant or a footgun, since
   a departure rule that resubscribes would resurrect a scope the user just
   closed; (c) cascade order when a departure rule writes into a rel that is
   itself under the dying scope.
5. **The salt for repeat.** Lab: a fresh integer per subscription. LANG.md's
   open question proposes an arrival-tick salt instead; that would make two
   same-tick resubscribes to one target collide into silence, which is exactly
   the failure the salt exists to prevent. Also open: whether the surface lets
   the user write a salt at all, or whether it is always engine-minted.
6. **Where does a rule's scope come from?** `switch_scope` names it by literal
   id here. Surface needs lexical binding (the enclosing subscription of the
   rule's island). Until that exists, scopes cannot nest more than the lab's
   two levels in real program text.
7. **Unknown-salt fill for a demanded target.** Lab drops it (the gate looks up
   the pair). The alternative, matching on `Target` alone, would let any late
   response satisfy any live request for the same endpoint, which is the SWR
   bug in disguise.
8. **`scope_done` is a rel the engine reads by name**, which the v5 magic-rel
   ban forbids. Either it becomes a declaration (`complete_when(Scope, Body)`)
   or the rel is registered the way `diag` is under ruling `a6_diag`.
9. **Per-scope retention.** Rows written under a scope survive it and are bound
   only by the rel's `keep` clause. A scope that writes 10k rows and dies leaves
   10k rows. Is per-scope retention wanted, or is rel-level `keep` the whole
   story? (The thesis says the latter, but nobody has costed a long UI session.)
10. **Do forest rows belong in the public delta stream?** They are rows, so r7
    emits them, and `teardown_is_visible_as_forest_deltas` grades that. A UI
    subscribed to "everything" would therefore see its own subscription rows
    appear and vanish. Filtering is a consumer concern today; it may want to be
    a declaration.

## Correspondence with mode_lab (the static claim, operational)

mode_lab's rule is `lifetime(inner) = min(own binding, enclosing scope)`, and
`every(300s)` alone is `never` while under `switch_map` it is `until(outer)`.
This lab runs that as two executions of ONE program with ONE canned timer:

- `dominated_timer`: the timer's subscription is a switch_map child. `complete(1)`
  at tick 5 deletes the prefix, the demand row goes, fills at ticks 6 and 7 are
  refused. Two poll rows.
- `undominated_timer`: the same timer also has a root subscription. The child
  scope is still torn down at tick 5, but the root demand row still supports
  `demanded(every300(session_one), 1001)`, so the fills land. Four poll rows.

`dominance_is_the_only_difference_between_the_two_runs` grades the difference
as exactly `{poll(session_one,3), poll(session_one,4)}`. So the static lifetime
lattice has an operational witness: **dominance is which prefix your demand row
sits under**, and nothing in the timer, the bind, or the rule changed between
the two runs. mode_lab can cite this file for case 4 of its grading contract.

## The UI worked example, end to end

Program: a list panel subscribing to a filtered/mapped level view; selecting an
item switch_maps a detail pane; selecting another swaps it; closing the panel
tears everything down.

```
visible_item(ItemId, Label) <- demanded(item_list, _), item(ItemId, Label, active);
detail_view(ItemId, Field)  <- demanded(detail(ItemId), _), detail_row(ItemId, Field);
ui_log(selected, ItemId)    <+ only(select(ItemId));
closed(ItemId)              <+ only(departed(visible_item(ItemId, _)));
switch_scope(select(ItemId), 1, detail(ItemId))
```

Trace (9 ticks, forest rows shown so the script is visible next to the data):

```
1  +demanded(item_list,7) +sub(1,0,item_list) +sub_path(1,[1]) +demand(1001,1,item_list,7)
   +visible_item(item_a,alpha) +visible_item(item_b,beta)
2  +item(item_c,gamma,active) +visible_item(item_c,gamma)
3  +select(item_a) +ui_log(selected,item_a)
   +sub(1001,1,detail(item_a)) +sub_path(1001,[1,1001]) +demand(1002,1001,detail(item_a),1001)
   +demanded(detail(item_a),1001)
4  +detail_row(item_a,body_a) +detail_view(item_a,body_a)
5  -detail_view(item_a,body_a) -sub(1001,...) -sub_path(1001,...) -demand(1002,...) -demanded(detail(item_a),1001)
   +select(item_b) +ui_log(selected,item_b)
   +sub(1002,1,detail(item_b)) +sub_path(1002,[1,1002]) +demand(1003,1002,detail(item_b),1002)
   +demanded(detail(item_b),1002)
6  +detail_row(item_b,body_b) +detail_view(item_b,body_b)
7  -visible_item(item_a,alpha) -visible_item(item_b,beta) -visible_item(item_c,gamma)
   -detail_view(item_b,body_b) -sub(1,...) -sub(1002,...) -sub_path(...) -demand(...) -demanded(...)
8  +closed(item_a) +closed(item_b) +closed(item_c)        (r4 departure carry, one drain tick)
9  []                                                      (quiet)
```

Final forest: `[]`. Final data: every `item` row (including the archived one
the view never showed), both `detail_row` bodies fetched during the session,
both `ui_log` occurrences, both `select` occurrences, three `closed` rows.
Final views: `visible_item = []`, `detail_view = []`.

Only deltas crossed the coastline. The whole session moved **10 view deltas**
(3 list rows in, 1 more in, 3 out at close, 2 detail rows in, 2 out on swap and
close, counted as 6 + 4). A per-tick scan of the same two views over the same 9
ticks could not have gone below 27 rows. `ui_delta_fold_reconstructs_the_view`
grades the stronger statement: folding the delta stream at every prefix length
1..6 reproduces exactly the rowset a table scan would have returned, so the UI
never needed the table.

## What this means for the tier order

- `task(sub_graph_disk, unbuilt, [emit_ts_direct])` in ARCH.pl now has a
  schema, a teardown statement shape, and 41 graded behaviors to conform to.
- `task(mode_lab, unbuilt, [])` gets its runtime witness for case 4; the static
  fold and this interpreter must agree on which subscriptions are `until(S)`.
- The RXJS FIRST law makes these ticks marble frames: teardown is an
  unsubscribe frame, `switch_map` is the standard marble, and the per-tick
  delta lists are the oracle the js leg reuses. The one js-side shape this
  forces is that the single terminal subscription must expose scope teardown as
  a row delete, not as `Subscription.unsubscribe()`, or the two legs will
  disagree the first time a stale response arrives.
- Ambiguity 1 (stale fill) blocks the effect-rel lowering, not this lab: SWR
  cache semantics depend on it. It is the one item here that should reach the
  user before the effect tier is written.
