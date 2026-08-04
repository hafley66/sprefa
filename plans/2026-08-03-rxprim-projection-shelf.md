# rxprim projection shelf (2026-08-03, SHELVED — wake on demand)

Coordinator sketch frozen mid-thread at the user's word ("we'll come back to
it later"). Three plan lanes (rxprim-kimi / rxprim-flash / rxprim-opus, same
CONTRACT.md, bus m-e1bbdfd8 / m-be5ecd82 / m-a74ab3cf) were in flight at
shelving; their PLANs supersede this sketch where they disagree. Everything
marked ▲ is INVENTED spelling; landed grammar is only key(1) / log keep(all)
/ keep(count(N)).

## 1. The 3-axis marble notation (proposed fixture-header form)

```
(tick, round, arrival-index)
 tick          = one task-queue drain (js sync run-to-completion). Columns.
 round         = one pass of the within-tick derivation fixpoint (rx expand).
                 Sub-columns r1 r2 r3. The axis classic marbles lack.
 arrival-index = order inside the batch. Top-to-bottom inside a cell.
 Rows:  E edge (only marks)   L level (marks + state at tick end)   G log (appends)
 Marks: +x add   -x del   (+x -y same cell = keyed replacement)   ∅ nothing
 ⌁ settle = after last round: retention prune, level sample.
```

Teaching pair: one arrival stream (1,'a') then (1,'b') in one tick reads as
E: two marks; G: two appends kept forever; L key(1): `+1,a -1,a +1,b`, state
{1:b}. All three are groupBy+scan with different folds:

| decl | fold | state vs history |
|---|---|---|
| key(1) | scan((_, row) => row) | differ |
| log keep(all) | scan((hist, row) => [...hist, row]) | differ |
| log keep(count(N)) | scan((win, row) => [...win, row].slice(-N)) | differ |
| first-wins | scan((acc, row) => acc ?? row) | IDENTICAL (why log-vs-key is vacuous for one()) |

## 2. Open engine problems in the notation (measured, COMPOSE.md)

1a add-only fixpoint truncation (emitter runs r1 only; departure path has the
full WITH RECURSIVE machinery):

```
             │ t1 batch +1,2 +2,3 +3,4                │ t2..t4
             │ ├─r1──────┬─r2──────┬─r3────┤          │
L reach ORCL │  +1,2      +1,3      +1,4              │ ∅ (frontier empty)
             │  +2,3      +2,4                        │
L reach EMIT │  +1,2     ██ r2 never runs ██          │ ∅ (never resumes)
             │  +2,3 +3,4
final ORCL {12,13,14,23,24,34}   EMIT {12,23,34}   both emitter modes agree = parity blind
```

1b golden-flex fold wall: third link never lands (t6/t7/t9 all measured
identical), minimal two-rule program chains to depth 4 fine; reconcile_every_tick
hypothesis falsified; cause open.

3 edge arm on level-headed trigger drops retractions: 10 adds with 6
same-batch content-key replacements -> oracle nets 4 tickets/4 log appends,
emitter keeps 10/16. Delete the edge arm = byte-identical doors.

one-race (pre-ruling): oracle picks by arrival index, emitter by source arm
order (concat-of-arms). Four-run table in COMPOSE.md section 3.

## 3. Rulings landed this thread (rulings.pl is authority)

- one_pick_order: winner = arrival order within the tick, BOTH doors. Emitter
  must stop consulting source arm order (merge, never concat).
- one_admission_no_lockout: first-wins fold AND one-per-tick takeover both
  stay sound; ruling one() = first-wins forecloses nothing.
- one_decl_surface: constructs land as rel-declaration properties beside
  key(1)/keep() so every existing decl checker sees them natively. Standing
  note inside it: keyed-vs-log split disliked, revisit later, no feature may
  deepen it meanwhile.

## 4. Clock-checker model (read from 3_clock_check.pl, receipts at lines)

Every rule body atom contributes dependency(RuleId, From, To, ReadRing,
WriteRing, Sign, Grade, Role) (:34). Roles: trigger / level_read /
level_absence / edge_sample / edge_pre / edge_absence / edge_departure.
Grades: edge-headed trigger 1, level read 0, pre -1. Causal roles only
(level_read, level_absence, trigger, edge_departure, finalize_in_level)
advance the inferred clock (:158); samples constrain, never schedule.
inferred_clock sums grades from origin rels (:166). Cycles legal when
productive (every simple cycle positive total grade) or constructive (:207).
The one order-dependent shape is labelled
not_provable(arm_absence_batch_invariance(...)), never refused (:224).

Merge consequence: N arms on one head = N in-edges on one node; the head's
clock is downstream of every arm automatically. A merge construct costs the
checker NOTHING; only decl-level conflict checks (edge_head_conflict_risk,
retention_head_conflict_risk) have opinions about colliding arms.

## 5. The projected family (▲ = invented, none parse today)

| construct | decl slot | rx word | new checker machinery |
|---|---|---|---|
| any | exists (two arms, log keep(all)) | merge | none |
| ▲ one | keep(first) | merge + take(1) | none; loser mark rides retention_prune_is_a_visible_minus |
| ▲ serializer | zip(tick) | zip(perKey, ticks$) | one self-edge grade 1, existing productive-cycle class |
| ▲ typed merge | enum tag column, one literal per arm | merge of tagged maps | none |
| pick order | semantics, no spelling | merge never concat | emitter change only |

Sketches:

```dl
rel dispatch_winner(dispatch_id: int, note_tag: text) log keep(first).   ▲
rel active_speaker(channel_id: int, speaker: text) key(1) zip(tick).     ▲
```

```
keep(first):  │ t1: +1,beta +1,alpha │ t2: +1,alpha
G winner      │ +1,beta (alpha pruned│ ∅ (guard reads settled {1:beta},
              │  visibly at ⌁)       │    refused visibly)

zip(tick):    │ t1: +7,ana +7,raj +7,mei │ t2         │ t3
L active      │ +7,ana (raj, mei queued) │ -7,ana +7,raj │ -7,raj +7,mei
```

Pattern the projection keeps landing on: every construct is a retention
policy, an admission policy, or a tag column — three slots the decl line
already has or can grow without a new statement type (one_decl_surface).

## 6. Parked adjacent

- Visual merge tool (user: "rad, dont immediately need"): marble view rendered
  from a real tick log; home = flow panel or fixture HTML report.
- Deferral insight worth keeping: admission-per-tick makes the negation guard
  deterministic because it reads SETTLED prior-tick state; the exact shape the
  checker labels arm_absence_batch_invariance becomes sound.
