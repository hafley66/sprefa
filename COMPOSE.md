# COMPOSE.md

What happened when enum, match, a scan-shaped fold, `decode/2` json and nested
values were made to hold hands inside one program, and where `one` sits next to
`any`. Every claim here is a command that was run in this worktree.

Landed in `v6/dl/fixtures/golden-flex.dl6` (the composition scenario section),
`v6/tsv2/scripts/golden-schedules.ts` (its arrivals),
`v6/prolog/conformance/fixtures/one_vs_any.pl` (the gap ledger) and
`v6/prolog/compile/test/3_clock_check.test.pl`
(`two_arm_negation_guard_is_a_race_not_one`).

Gates at the end of the lane:

    just conformance   289 PASS, 0 fail, exit 0        (285 before)
    just plunit        322/322, exit 0
    just text-door     TEXT_DOOR compiled=202 byte_identical=202 failures=0
    just golden-flex   GOLDEN FLEX HOLDS, exit 0
                       zero 6 / one 120 / many 5891 / perturbed 5942 final row
                       groups (91 / 4177 / 4211 before)

---

## 1. What composed cleanly

### json into a nested capture

```dl
rel dispatch_route(dispatch_id: int, route: text, hops: int).
dispatch_route(DispatchId, RouteName, HopCount) <-
  dispatch_manifest(DispatchId, ManifestPayload),
  decode(ManifestPayload, {route: {name: RouteName, hops: HopCount: int}}).
```

rx lowering:

```ts
const dispatchRoute$ = dispatchManifest$.pipe(
  mergeMap(({ dispatchId, payload }) => {
    const route = payload?.route;
    return route !== undefined && typeof route.hops === "number"
      ? of({ dispatchId, route: route.name, hops: route.hops })
      : EMPTY;
  }),
);
```

The pattern is a projection whose failure is an empty emission, so `mergeMap`
into `EMPTY` is the whole story. The file already had single-capture patterns,
`**` descent and `$key` capture. Two captures at depth two in one pattern, one
plain and one typed, worked on the first try and needed no new grammar.

### enum with a tag column, read by a match block

```dl
rel dispatch(air(hours: int) ; road(miles: int) ; rail(stops: int)).

rel dispatch_ticket(dispatch_id: int, tag: text, route: text, hops: int).
dispatch_ticket(DispatchId, DispatchTag, RouteName, HopCount) <-
  dispatch_tag(DispatchId, DispatchTag),
  dispatch_route(DispatchId, RouteName, HopCount).

rel dispatch_plan(dispatch_id: int, route: text, verdict: text).
match dispatch_ticket(DispatchId, DispatchTag, RouteName, HopCount) (
  ; DispatchTag == 'air'  |-> dispatch_plan(DispatchId, RouteName, "fly it")
  ; DispatchTag == 'road' |-> dispatch_plan(DispatchId, RouteName, "truck it")
  ; HopCount > 2          |-> dispatch_plan(DispatchId, RouteName, "stage it")
).
```

rx lowering:

```ts
const dispatchTag$ = merge(
  dispatchAir$.pipe(map((row) => ({ dispatchId: row.dispatchId, tag: "air" }))),
  dispatchRoad$.pipe(map((row) => ({ dispatchId: row.dispatchId, tag: "road" }))),
  dispatchRail$.pipe(map((row) => ({ dispatchId: row.dispatchId, tag: "rail" }))),
);

const dispatchTicket$ = combineKeyed(dispatchTag$, dispatchRoute$, (t) => t.dispatchId);

const dispatchPlan$ = merge(
  dispatchTicket$.pipe(filter((t) => t.tag === "air"),  map((t) => verdict(t, "fly it"))),
  dispatchTicket$.pipe(filter((t) => t.tag === "road"), map((t) => verdict(t, "truck it"))),
  dispatchTicket$.pipe(filter((t) => t.hops > 2),       map((t) => verdict(t, "stage it"))),
);
```

`merge` is the honest lowering of a match block because arms are independent
rules. `0_match_expand.pl:expand_match_arm/3` writes one `ArmHead <- SourceAtom,
Guards` per arm and adds no negated guard from earlier arms, so overlapping
guards all fire. The existing `graded` block in the golden demonstrates that:
its `true` catch-all rides on top of every ripe row, and `graded` really does
carry `[[1,"compost"],[1,"wait"]]` for one grade. This block keeps its guards
disjoint by feeding hops above two only to the rail variant, and says so at the
site. Anyone who reads `match` as rx `partition` or as a switch statement is
reading it wrong.

The composition that mattered: the scrutinee is a JOIN of the enum's tag view
with the json-decoded route, so one arm set guards on the enum tag and on a
number that came out of a json document. That needed nothing new either.

### struct value carried into the new flow

```dl
rel dispatch_leg(leg_id: int, dispatch_id: int, previous_leg: int, kilos: int,
                 origin: patch).

rel leg_origin(leg_id: int, label: text).
leg_origin(LegId, OriginLabel) <-
  dispatch_leg(LegId, _DispatchId, _PreviousLeg, _Kilos, Origin),
  decode(Origin, {label: OriginLabel}).
```

rx lowering: `dispatchLeg$.pipe(map((leg) => ({ legId: leg.legId, label:
leg.origin.label })))`. `patch` is the golden's existing struct type, whose `at`
is a `plot`, so the scenario reuses the file's two-hop dictionary chain instead
of inventing a flat column. Struct values still arrive whole, because a braces
literal in a rule is `unsupported_construct(json_value_expression(...))`, which
the golden's header already records.

### two arms on one head, same tick, both landing

```dl
rel dispatch_note(dispatch_id: int, note_tag: text) log keep(all).
dispatch_note(DispatchId, 'acked')  <+ dispatch_ack(DispatchId).
dispatch_note(SealedId, 'sealed')   <+ dispatch_seal(SealedId).
```

rx lowering:

```ts
const dispatchNote$ = merge(
  dispatchAck$.pipe(map((row) => ({ dispatchId: row.dispatchId, noteTag: "acked" }))),
  dispatchSeal$.pipe(map((row) => ({ dispatchId: row.dispatchId, noteTag: "sealed" }))),
);
```

`merge` with no operator after it. That is `any`, and it is the only member of
the any/one pair the surface has.

---

## 2. The fold, and the two walls it sits between

```dl
rel leg_total(leg_id: int, dispatch_id: int, kilos_so_far: int).
leg_total(LegId, DispatchId, Kilos) <-
  dispatch_leg(LegId, DispatchId, 0, Kilos, _Origin).
leg_total(LegId, DispatchId, KilosSoFar) <-
  dispatch_leg(LegId, DispatchId, PreviousLeg, Kilos, _Origin),
  leg_total(PreviousLeg, DispatchId, KilosBefore),
  KilosSoFar := KilosBefore + Kilos.
```

Two rx lowerings, and which one is correct depends on how the legs arrive.

One leg per tick (a stream) is `scan`:

```ts
const legTotal$ = dispatchLeg$.pipe(
  scan((totalByLeg, leg) => {
    const before = leg.previousLeg === 0 ? 0 : totalByLeg.get(leg.previousLeg) ?? 0;
    return new Map(totalByLeg).set(leg.legId, before + leg.kilos);
  }, new Map<number, number>()),
);
```

A whole chain in one batch is `expand`, since the rule keeps re-firing on its
own output until the frontier empties:

```ts
const legTotal$ = seedLegs$.pipe(
  expand((total) => legsAfter(total.legId).pipe(
    map((leg) => ({ legId: leg.legId, kilosSoFar: total.kilosSoFar + leg.kilos })),
  )),
);
```

The reference engine implements `expand`; the emitted module runs one round of
it and stops.

### Finding 1a: a chain inside one tick runs one round in the emitter

Program (`supply_depth` is the same fold shape with a plain closure beside it):

```dl
rel supply_link(upstream: int, downstream: int).
rel supply_reach(upstream: int, downstream: int).
supply_reach(UpNode, DownNode) <- supply_link(UpNode, DownNode).
supply_reach(UpNode, DownNode) <-
  supply_reach(UpNode, MiddleNode),
  supply_link(MiddleNode, DownNode).
```

Arrivals: one batch, `[1,2] [2,3] [3,4]`.

    oracle    {"final":{...,"supply_reach":[[1,2],[1,3],[1,4],[2,3],[2,4],[3,4]]}}
    emitter   {"final":{...,"supply_reach":[[1,2],[2,3],[3,4]]}}

Both emitter modes agree with each other and disagree with the oracle, so
emitter-mode parity cannot see this. Three further empty ticks do not advance
it:

    {"tick":1,"deltas":{...,"supply_reach":{"add":[[1,2],[2,3],[3,4]],"del":[]}}}
    {"tick":2,"deltas":{}}
    {"tick":3,"deltas":{}}
    {"tick":4,"deltas":{}}
    {"final":{...,"supply_reach":[[1,2],[2,3],[3,4]]}}

One thing does advance it: a DEPARTURE. Delete `[3,4]` on tick 2 and the
refCount reconcile runs, and that path is emitted as a real `WITH RECURSIVE`
CTE, so the answer becomes the full closure of what is left:

    {"tick":2,"deltas":{"supply_link":{"add":[],"del":[[3,4]]},
                        "supply_reach":{"add":[[1,3]],"del":[[3,4]]}}}
    {"final":{"supply_link":[[1,2],[2,3]],"supply_reach":[[1,2],[1,3],[2,3]]}}

So the closure machinery is emitted and correct; the add-only path does not
reach it. Nothing in the corpus covered this: `flagship-flow.dl6` carries a
transitive closure and `scripts/flagship-callgraph.sh:63` says in so many words
that `reaches` "is not ported and not graded", and the conformance fixture
`flagship_flow_reach_over_resolved_edges` is graded against the reference engine
only.

### Finding 1b: inside golden-flex the fold stops at two links, cause not isolated

The scenario originally carried three legs per dispatch, arriving on ticks 2, 3
and 6, one link per tick. The third never landed in the emitter:

    oracle    leg_total [[11,1,2],[12,1,5],[13,1,9]]
    emitter   leg_total [[11,1,2],[12,1,5]]

Moving the third leg to tick 7 or to tick 9 changes nothing; all three
placements were run and all three give the same two rows. Meanwhile the same
fold shape in a two-rule program, fed one link per tick, chains as far as it is
fed and both doors agree:

    oracle    supply_depth [[1,0],[2,1],[3,2],[4,3],[5,4]]
    emitter   supply_depth [[1,0],[2,1],[3,2],[4,3],[5,4]]

So the wall belongs to something the big program has. FIRST GUESS, MEASURED AND
FALSIFIED: `emit_ts.pl`'s `reconcile_every_tick` fires when any level rule has a
negated body ref, and golden-flex has one (`pickable` reads
`not(quarantined(Species))`), which would make every tick reconcile every
non-aggregate level statement. Adding a negated level body to the two-rule
program does not reproduce the ceiling; it still chains to depth 4. The cause is
open.

Consequence in the landed file: the fold is two links on two ticks, and both
numbers are commented at the site as measured walls rather than as taste.

---

## 3. any and one

`any` is spelled. `one` is not, and the three honest attempts at it fail in
three different ways. The ledger is
`v6/prolog/conformance/fixtures/one_vs_any.pl`, committed fail-first (the
expectations written as the wish) and then corrected to the measurement, so the
diff between those two commits IS the gap.

The fail-first run, verbatim:

    MISMATCH deltas dispatch_note/2
      got [[+dispatch_note(1,acked),+dispatch_note(1,sealed)],[]]
      want [[+dispatch_note(1,acked),+dispatch_note(1,sealed)]]
    fail  any_two_tagged_arms_land_on_one_tick
    MISMATCH deltas dispatch_winner/2
      got [[+dispatch_winner(1,sealed)],[]]
      want [[+dispatch_winner(1,acked)]]
    fail  one_attempt_keyed_head_loses_the_first_arm_silently
    ERROR: [Thread main] Unknown message: retention_head_conflict_risk(dispatch_first/2,count(1))
    fail  one_attempt_bounded_log_two_arms_refused
    MISMATCH deltas dispatch_first/2
      got [[+dispatch_first(1,acked)],[]]
      want [[+dispatch_first(1,sealed)]]
    fail  one_attempt_guard_by_negation_lands_one_unnamed_winner
    FAILURES  4

`any` misses only the settle tick. The three `one` attempts follow.

### Attempt 1: a keyed head. The loser is not reported.

```dl
rel dispatch_winner(dispatch_id: int, note_tag: text) key(1).
dispatch_winner(DispatchId, 'alpha') <+ alpha_ping(DispatchId).
dispatch_winner(DispatchId, 'beta')  <+ beta_ping(DispatchId).
```

    {"tick":1,"deltas":{"alpha_ping":{"add":[[1]],"del":[]},
                        "beta_ping":{"add":[[1]],"del":[]},
                        "dispatch_winner":{"add":[[1,"beta"]],"del":[]}}}
    {"final":{...,"dispatch_winner":[[1,"beta"]]}}

Both doors agree on `beta`. The `alpha` row is in no add list, in no del list
and in no refusal: a reader of the tick log cannot tell that two arms fired.
`edge_head_conflict_risk` does not catch it because that check requires the arms
to share a trigger ref (`analyze.pl:check_no_edge_head_conflict_risk/2`), and
these have two.

rx lowering, which is exactly what the language did:

```ts
const dispatchWinner$ = merge(alphaArm$, betaArm$).pipe(
  groupBy((row) => row.dispatchId),
  mergeMap((group) => group.pipe(scan((_previous, row) => row))),
);
```

`scan((_previous, row) => row)` is last-write-wins, and it throws the previous
value away without telling anyone.

### Attempt 2: a bounded log. Refused.

```dl
rel dispatch_first(dispatch_id: int, note_tag: text) log keep(count(1)).
dispatch_first(DispatchId, 'alpha') <+ alpha_ping(DispatchId).
dispatch_first(DispatchId, 'beta')  <+ beta_ping(DispatchId).
```

Text door, verbatim:

    {"code":"retention_head_conflict_risk/2","message":"bounded.dl6:4: unsupported_construct:
     compiler refused rule 'retention_head_conflict_risk' for rel 'dispatch_first/2'
     (retention_head_conflict_risk)","range":{"end":{"character":0,"line":3},
     "start":{"character":0,"line":3}},"severity":1,"source":"dl6",...}
    refusal: bounded.dl6:4: unsupported_construct: compiler refused rule
     'retention_head_conflict_risk' for rel 'dispatch_first/2'
     (retention_head_conflict_risk)

Term door, verbatim:

    ERROR: [Thread main] Unknown message: retention_head_conflict_risk(dispatch_first/2,count(1))

Ruling `bounded_log_arm_order` in `conformance/rulings.pl`, user 2026-08-03:
"refuse it". The refusal is the right call and it is also the reason this
spelling cannot become `one`: retention prunes at tick end, so the survivor
would be whichever arm ran last, and arm order is source line order.

rx lowering of what was refused:

```ts
merge(alphaArm$, betaArm$).pipe(
  groupBy((row) => row.dispatchId),
  mergeMap((group) => group.pipe(scan((window, row) => [...window, row].slice(-1), []))),
);
```

### Attempt 3: guard by negation. It compiles, and the two doors referee it differently.

```dl
rel dispatch_first(dispatch_id: int, note_tag: text) log keep(all).
dispatch_first(DispatchId, 'alpha') <+
  alpha_ping(DispatchId), not(dispatch_first(DispatchId, _AlphaTag)).
dispatch_first(DispatchId, 'beta') <+
  beta_ping(DispatchId), not(dispatch_first(DispatchId, _BetaTag)).
```

The clock checker does NOT refuse this. It names a non-refusing boundary, once
per arm, verbatim:

    boundaries: [not_provable(arm_absence_batch_invariance(rule(1,edge,dispatch_first/2),dispatch_first/2)),
                 not_provable(arm_absence_batch_invariance(rule(2,edge,dispatch_first/2),dispatch_first/2))]

`check_clock_program/1` passes. `3_clock_check.pl` states the reason in its own
comment: the one shape that is genuinely order dependent closes through the
arm's own edge-headed head, and it is labelled rather than refused because a
ruled fixture rides it.

Then the measurement, four runs, same rows, arm order and arrival order each
flipped once:

| arm order in source | arrival order | oracle | emitter |
| --- | --- | --- | --- |
| alpha, beta | alpha, beta | alpha | alpha |
| alpha, beta | beta, alpha | **beta** | **alpha** |
| beta, alpha | alpha, beta | **alpha** | **beta** |
| beta, alpha | beta, alpha | beta | beta |

The reference engine decides by ARRIVAL order. The emitted module decides by
SOURCE ARM order. They agree only when the two orders coincide, and nothing
today grades the shape, because no fixture in the corpus puts two
negation-guarded arms on one head.

rx lowering, one per door, which is the cleanest statement of the divergence:

```ts
// reference engine
merge(alphaArm$, betaArm$).pipe(groupBy(byId), mergeMap((g) => g.pipe(take(1))));
// emitted module
concat(alphaArm$, betaArm$).pipe(groupBy(byId), mergeMap((g) => g.pipe(take(1))));
```

`merge` versus `concat`, and the rest of the pipeline identical.

The conformance ledger therefore grades only the agreeing order, because a
fixture is swept through both doors. The disagreement is pinned on the oracle
side by `two_arm_negation_guard_is_a_race_not_one` in
`compile/test/3_clock_check.test.pl`, which asserts both boundary terms and both
arrival orders, and which carries a sabotage receipt.

### What `one` would have to be

rx has five different words for this and the language has none of them: `race`,
`first`, `take(1)`, `exhaustMap`, `switchMap`. Whatever `one { }` turns out to
mean, the four fixtures in `one_vs_any.pl` are what it has to change, and the
`merge` versus `concat` split above is what it has to settle first: today the
two engines do not agree on what "the first one" means.

---

## 4. Finding 3: an edge arm triggered by a level-headed rel loses retractions

Found while building the `any` receipt. The first spelling of the second arm
triggered off the enum's tag view rather than off an arrival rel, and the 100
row leg of the golden went red. Minimal repro, ten lines and two batches:

```dl
rel dispatch(air(hours: int) ; road(miles: int)).
rel dispatch_route(dispatch_id: int, route: text).
rel dispatch_ticket(dispatch_id: int, tag: text, route: text).
dispatch_ticket(DispatchId, DispatchTag, RouteName) <-
  dispatch_tag(DispatchId, DispatchTag),
  dispatch_route(DispatchId, RouteName).
rel dispatch_note(dispatch_id: int, note_tag: text) log keep(all).
dispatch_note(DispatchId, 'tagged') <+ dispatch_tag(DispatchId, _DispatchTag).
```

Arrivals: tick 1 `dispatch_route(i, "ri")` for i in 1..10; tick 2
`dispatch_air(i, i mod 4)` for i in 1..10. An enum variant rel is keyed on its
CONTENT columns (`0_enum_expand.pl:content_key_positions/2` gives positions
2..N), so ten ids over four content values means six same-batch replacements.

    oracle   dispatch_ticket [[10,"air","r10"],[7,"air","r7"],[8,"air","r8"],[9,"air","r9"]]
             dispatch_note   [[10,"tagged"],[7,"tagged"],[8,"tagged"],[9,"tagged"]]

    emitter  dispatch_ticket [[1,"air","r1"],[10,"air","r10"],[2,"air","r2"],[3,"air","r3"],
                              [4,"air","r4"],[5,"air","r5"],[6,"air","r6"],[7,"air","r7"],
                              [8,"air","r8"],[9,"air","r9"]]
             dispatch_note   [[1,"tagged"],[1,"tagged"],[2,"tagged"],[2,"tagged"],
                              [3,"tagged"],[3,"tagged"],[4,"tagged"],[4,"tagged"],
                              [5,"tagged"],[5,"tagged"],[6,"tagged"],[6,"tagged"],
                              [10,"tagged"],[7,"tagged"],[8,"tagged"],[9,"tagged"]]

Both doors agree on `dispatch_air` and on `dispatch_tag`. They disagree on the
derived join and on the log. Delete the one edge arm and the same schedule
against the same program is byte identical on both doors, so the edge arm is
what carries it. The emitter keeps the six retracted tickets and double-fires
the log for the six replaced ids.

Consequence in the landed file: the golden's `any` arms trigger off two arrival
rels, and the `dispatch` variant content is unique per index so the scenario
never takes a replacement. Both choices are commented at their sites with this
reason.

---

## 5. What grated, in one list

| grated | receipt |
| --- | --- |
| `match` arms are independent rules, so guards must be made disjoint by hand or every matching arm fires | `0_match_expand.pl:expand_match_arm/3`; the golden's own `graded` carries `[[1,"compost"],[1,"wait"]]` for one grade row |
| an enum variant rel is keyed on its CONTENT, so two ids with equal content silently replace each other | `0_enum_expand.pl:content_key_positions/2`; at 100 rows a four-value content column left four tickets out of a hundred |
| a self-referential level rule is `expand` in the reference engine and one round in the emitter | finding 1a |
| the same rule stops at two links inside a large program for a reason that did not minimize | finding 1b |
| an edge arm on a level-headed trigger loses that rel's retractions in the emitter | finding 3 |
| `one` has no spelling, and the closest attempt is refereed differently by the two doors | finding 2, section 3 |
| struct values still cannot be built in a rule, so every nested value arrives whole | pre-existing, `json_value_expression` in the golden's header |

## 6. Best translation fit, for the design lane

| construct | rx word it lowered to | confidence |
| --- | --- | --- |
| `decode/2` pattern | `mergeMap` into `of(...)` or `EMPTY` | high, exact |
| enum tag view | `merge` of per-variant `map` | high, exact |
| `match` block | `merge` of independent `filter`+`map` arms | high, and the surface reads like `partition`, which it is not |
| self-referential level rule, one element per tick | `scan` | high |
| self-referential level rule, whole chain per tick | `expand` | high for the reference engine, unimplemented in the emitter |
| two edge arms on `log keep(all)` | `merge` | high, exact, and this IS `any` |
| keyed edge head | `groupBy` + `scan((_, row) => row)` | high, exact |
| `log keep(count(N))` | `groupBy` + windowed `scan` | refused with two arms |
| negation-guarded arms | `merge` + `take(1)` in the oracle, `concat` + `take(1)` in the emitter | high, and the two are not the same operator |
