# UPDATE-ARM LAB VERDICT (2026-07-29)

## Verdict line

**The zero-construct hypothesis HOLDS on semantics and BREAKS on the spelling
the header wrote.** The OLD/NEW update arm needs no new construct: an EDGE rule
whose trigger is `finalize(old row)` and whose join is the current table
produces exactly one `(key, old, new)` row per boundary transition, over both a
keyed edge-headed rel and a plain derived level view. What breaks is the
header's literal rule, which used a LEVEL arrow: `engine.pl:113-114` refuses
`finalize` in any `<-` rule at load time with `finalize_in_level_rule/1`, so the
hypothesis as written throws before it runs. Change `<-` to `<+` and every case
holds.

Three real cracks sit under the holding verdict, all pre-existing and all
silent: a pure delete produces nothing unless the author writes a second arm
with `not(current(...))`; the arm over a Log rel is statically dead with no
refusal; and the compiled path refuses the whole family
(`edge_body_needs_finalize`, 2 of 60 unsupported fixtures).

Lab: `v6/prolog/labs/update_arm/lab.pl`, 19 checks, 19 PASS twice, exit 0,
PASS-only stdout, every check driven by the real oracle through
`run_program/5`. Six sabotage probes red as required (receipts in the lab
header).

## The two spellings

Refused, the header's:

```
changed_value(key, old_value, new_value) <-
    finalize(current_value(key, old_value)), current_value(key, new_value).
```

Accepted, and the subject of every case below:

```
current_value(key, value) <+ poll_value(key, value).

changed_value(key, old_value, new_value) <+
    finalize(current_value(key, old_value)), current_value(key, new_value).
```

Why the refusal is right rather than an accident: a level rule has no
occurrences at all. `engine.pl:286` runs `level_closure/5` over the whole base
each tick, so there is no "the departure fired" event a level body could bind.
The refusal is `check_program/1`'s, at load, before any tick.

## Per-case table

| case | scenario | result | receipt |
|---|---|---|---|
| U0 | header's literal `<-` spelling | **REFUSED at load**, `finalize_in_level_rule(current_value/2)` | `u0_literal_level_rule_spelling_is_refused` |
| U0 | same rule with `<+` | loads and runs | `u0_edge_rule_spelling_loads` |
| U1 | keyed replace v1 -> v2 | **exactly one row** `changed_value(cli,v1,v2)` | `u1_keyed_replace_yields_one_pair` |
| U1 | tick placement | **replace tick PLUS ONE.** Replace lands at tick 2, arm fires at tick 3, 4 ticks total | `u1_arm_fires_the_tick_after_the_replace` |
| U1c | arm over a DERIVED level view, no `keyed` decl | **works**, one row, same +1 placement | `u1c_the_arm_works_over_a_derived_level_view` |
| U2 | plain insert, no prior row | **arm silent**, 2 ticks, zero `changed_value` deltas | `u2_plain_insert_leaves_the_arm_silent` |
| U3 | plain delete, no successor | **arm silent.** Departure fires; the current-side join finds nothing; no row, no diagnostic | `u3_plain_delete_leaves_the_arm_silent` |
| U3b | delete arm via `not(current(...))` | **separable with zero constructs**: `deleted_value(cli,v1)` on the delete, empty on the replace | `u3b_delete_is_separable_by_negating_the_current_side`, `u3b_replace_does_not_trip_the_delete_arm` |
| U4 | same-tick v1 -> v2 -> v3 | **one row, the honest endpoint pair** `changed_value(cli,v1,v3)` | `u4_same_tick_double_replace_yields_the_endpoint_pair` |
| U4b | same-tick v1 -> v2 from EMPTY | **zero rows.** No minus delta, so no departure at all | `u4b_same_tick_replaces_from_empty_yield_nothing` |
| U5 | arm over a Log rel | **silently dead.** No refusal, no warning, `changed_event` empty | `u5_log_rel_arm_is_silently_dead` |
| U5b | Log rel under `keep(count(1))` | prune removes the row and emits **no delta of any kind**; arm still silent | `u5b_retention_prune_emits_no_departure` |
| U6 | `finalize` inside a match arm body | **composes.** Source atom becomes the join, `finalize` stays the trigger | `u6_match_block_composes_into_the_update_arm` |
| U6 | block form vs hand-written rule | **byte-identical** delta ticks and final state | `u6_match_block_tick_log_matches_the_hand_written_rule` |
| U6b | `finalize` in a LEVEL match arm | **refused after expansion**, `finalize_in_level_rule(current_value/2)` | `u6b_finalize_in_a_level_match_arm_is_refused` |
| U7 | compiled path | **UNSUPPORTED bucket**, named refusal `edge_body_needs_finalize/1` | sweep stage 1, below |

### U4 collapse semantics, observed verbatim

Program: `current_value` keyed on column 1, seeded `current_value(cli, v1)`,
schedule one tick `[ +poll_value(cli, v2), +poll_value(cli, v3) ]`.

```
final(changed_value/3)  = [changed_value(cli,v1,v3)]
final(current_value/2)  = [current_value(cli,v3)]
deltas(current_value/2) = [[-current_value(cli,v1), +current_value(cli,v3)], [], []]
deltas(changed_value/3) = [[], [+changed_value(cli,v1,v3)], []]
ticks                   = 3
```

So: **the honest endpoint pair, exactly one row.** Not a phantom `(v1,v2)`, not
a phantom `(v2,v3)`, not two rows. The three rejected readings were each run as
a sabotage probe and each came back with `got [changed_value(cli,v1,v3)]`.

Citation for the collapse: `engine.pl:299-304` (the R2 rider, carry-out is
boundary-observable writes only) plus the set-diff boundary at
`engine.pl:322-337`. The intermediate `current_value(cli,v2)` is written by the
first occurrence and replaced by the second inside one tick, so it never
appears in a delta and never becomes a carry occurrence. The departure carry is
minted from the tick's minus deltas at `engine.pl:308-312`, which sees only
`-current_value(cli,v1)`.

This settles the match-frontier lab's C2 "loses N-1 of N intra-tick
transitions" as a DEFINED semantics rather than a defect: the arm reports the
tick's net transition. What stays a real trap is the U4b row, where the same
two writes report NOTHING because the rel was empty at tick start. Firing count
is a function of the tick-start state, not of the data.

## SUGAR-SCOPE result

**Arm scope = the trigger atom's bindings plus the arm's own body. Sibling arms
share nothing.** Graded on the LANDED match block, three checks:

| check | shape | result |
|---|---|---|
| S1 | arm reads a trigger column (`Status == 200` over `match(resp(Endpoint, Status), ...)`) | works, `ok_endpoint(api)` / `bad_endpoint(worker)` |
| S2 | arm names a variable only its SIBLING binds, in a head column | **throws `unbound_in_expression`** at head evaluation (`body.pl:23`) |
| S3 | same, in a BODY rel-atom column | **silently a fresh wildcard.** Two rows where a shared-scope reading gives one |

S3 verbatim: with `label(api, hot)`, `label(worker, cold)` and the sibling
variable `Tag` reused in arm two's `label(Other, Tag)`, the engine yields
`[echoed(api,api), echoed(api,worker)]`. A shared-scope reading would bind
`Tag = hot` and yield `[echoed(api,api)]` alone. Sabotage probe for the
one-row expectation came back red with the two-row `got`.

Mechanism, so the answer is not accidental: arms of one match block are one
prolog term, so a repeated variable name IS one variable in the source term.
Separation comes from the evaluator, not the expander. Edge rules are copied
per occurrence (`engine.pl:216` `copy_term/2`); level rules are re-unified per
`member/2` solution inside a `findall/3` (`level_eval.pl:146-150`), and
backtracking undoes any binding before the next rule is tried. Either way no
sibling binding survives.

The failure asymmetry is the finding worth carrying: naming a sibling's
variable is **loud in a head or expression position and silent in a body atom
position**, where it merely widens the join. Named SLOT-ARM-SIBLING-WILDCARD
below.

## Rx lowering for every graded spelling

Standing law: each `.dl` spelling shown carries the pure-rxjs lowering it
means. All four below are writable; none is a design defect.

| spelling | rx lowering | directness |
|---|---|---|
| `current_value(key, value) <+ poll_value(key, value).` (keyed edge head) | `poll$.pipe(groupBy(row => row.key))`, the inner's latest element IS the row for that key | DIRECT |
| `changed_value(key, old, new) <+ finalize(current_value(key, old)), current_value(key, new).` (U1) | `poll$.pipe(groupBy(r => r.key), mergeMap(perKey$ => perKey$.pipe(distinctUntilChanged((a,b) => a.value === b.value), pairwise(), map(([oldRow, newRow]) => ({key: oldRow.key, oldValue: oldRow.value, newValue: newRow.value})))))` | DIRECT. `pairwise()` IS the update arm; it emits nothing for the first element, which is U2 exactly. `distinctUntilChanged` is the equal-row no-op at `engine.pl:247-248` |
| the U4 tick collapse | insert a per-tick fold before `pairwise`: `perKey$.pipe(bufferWhen(() => tickBoundary$), filter(batch => batch.length > 0), map(batch => batch[batch.length - 1]), distinctUntilChanged(...), pairwise())` | DIRECT. Bare `pairwise()` would emit TWO rows for v1 -> v2 -> v3; the boundary `last` is what makes it one. The rx operator names the ruled collapse honestly |
| `deleted_value(key, old) <+ finalize(current_value(key, old)), not(current_value(key, _)).` (U3b) | not `pairwise`; it is the group's own end: `groupBy(r => r.key, {duration: g => g.pipe(takeUntil(retract$))})` then `g.pipe(takeLast(1))` | DIRECT, different operator. This is why delete and replace are two arms and not two branches of one |
| `changed_event(...) <+ finalize(event_log(...)), event_log(...).` over a Log rel (U5) | `NEVER`. An append-only subject has no removal to observe, and retention is a `take`/`bufferCount` window that drops silently | VACUOUS, which is the honest lowering of statically dead |
| match block arms (SUGAR-SCOPE) | one source multicast into N filtered branches, each branch a separate closure over the source row: `src$.pipe(share())` then N `.pipe(filter(...), map(...))`. Separate closures = no sibling variable sharing, which is exactly S2/S3 | DIRECT |

## U7: compiled-path status (documentation only, nothing changed)

Sweep stages 1 and 2 ran to completion in this worktree. Stage 3 could not run:
a fresh worktree has no `node_modules` and `v6/tsv2` has no lockfile, so the
diff leg needs a network install. **N/A-with-reason**, not a failure.

Stage 1, unchanged from the stated baseline:

```
SWEEP total=126 compiled=66 unsupported=60 crash=0
```

The finalize/departure family sits in the **UNSUPPORTED** bucket under one
named refusal:

```
UNSUPPORTED departed_fires_next_tick_on_retraction
              edge_body_needs_finalize((finalize(mirror(_)),now(_)))
UNSUPPORTED keyed_replace_departs_the_old_row
              edge_body_needs_finalize(finalize(latest(_,_)))
```

Source of the refusal: `registry.pl:18` carries `surface(finalize/1, time,
refs_of_arg(1, pos, trigger), wrapper(rel_atom, refuse(goal)), refused)`, and
`analyze.pl:606` turns any edge body containing it into
`edge_body_needs_finalize/1` at priority 2.

Bucket ranking of the 60 unsupported fixtures, so the arc can be sequenced
against real weight:

| count | refusal |
|---|---|
| 12 | `edge_body_needs_pre` |
| 6 | `edge_body_with_latest` |
| 6 | `edge_body_needs_negation` |
| 6 | `edge_body_needs_json_destructure` |
| 5 | `edge_body_needs_now` |
| 4 | `level_body_goal` |
| 4 | `aggregate_head` |
| **2** | **`edge_body_needs_finalize`** |
| 2 | `json_value_expression` |
| 2 | `edge_head_column_type_mismatch` |
| 1 each | `enum_variant_name_collision`, `match_nonexhaustive`, `keyed_level_head`, `keyed_log_rel`, `edge_into_unkeyed_set`, `arith_operand_not_int`, `join_column_type_mismatch`, `comparison_type_mismatch`, `decl_type_conflicts_witness`, `edge_head_conflict_risk`, `edge_body_needs_bind` |

Note the coupling this lab exposes: U3b's delete arm needs `not(...)` in an
edge body, which is `edge_body_needs_negation` (6 fixtures, priority 5). A
compiled update-arm family therefore needs the finalize seam AND the edge
negation seam before delete is expressible on the compiled path.

**Sweep footgun fired again** (already on the standing ledger as unowned): stage
3's `rm -f gen_emitted/*.ts` deleted `v6/tsv2/gen_emitted/door-handwritten.ts`,
which is not a fixture module. Caught by `git status` and restored with `git
checkout --` in the same sitting; the worktree ends clean. This is the fourth
recorded occurrence.

## Named slots

| slot | status | content |
|---|---|---|
| **SLOT-UPDATE-ARM** | **ANSWERED: per boundary, zero constructs** | The arm fires once per key per tick-boundary transition, at replace-tick plus one. Per-occurrence firing was never a live option; `engine.pl:308-312` mints departures from the tick's minus deltas, so occurrence granularity is not reachable without breaking the R2 rider. The SQL `AFTER UPDATE` OLD/NEW shape is already available under the existing `finalize` word |
| **SLOT-SUGAR-SCOPE** | **ANSWERED: trigger atom plus own body, never siblings** | S1/S2/S3. Sibling isolation is enforced by the evaluator (`copy_term` on edge rules, `findall` backtracking on level rules), not by the expander |
| SLOT-UPDATE-ARM-LEVEL-SPELLING | **OPEN, recommendation: keep the refusal** | Should the header's `<-` spelling ever be legal? A level rule has no occurrences, so making it legal means inventing a per-boundary firing concept for level rules. Recommend the refusal stays and the error message names the `<+` fix |
| SLOT-DELETE-ARM-DISCRIMINATION | **OPEN** | A pure delete is silent unless the author also writes the `not(current(...))` twin (U3 vs U3b). Both arms are cheap and correct, but nothing tells an author the first arm alone drops deletes. Options: leave it, warn when a `finalize` body joins the same rel without a negated twin, or add a `cause` column. This is the second half of the match-frontier lab's SLOT-CAUSE, now with a working answer for the first half |
| SLOT-LOG-FINALIZE-REFUSAL | **OPEN, recommendation: refuse** | `finalize` over a `kind(Ref, log)` rel is statically dead (U5) and decidable at load: `check_program/1` already has both `kind/2` and `body_finalize_ref/2` in hand. A one-line named refusal, same shape as the four already there. Retention (U5b) makes it worse, since a row genuinely leaves with no delta |
| SLOT-ARM-SIBLING-WILDCARD | **OPEN** | A variable appearing in exactly one match arm and unbound there is loud in a head position and silent in a body atom position (S2 vs S3). Should the expander refuse a singleton-in-this-arm variable that another arm binds? Cheap to check at expansion time; the cost is refusing a legitimately-anonymous variable that happens to collide by name |
| SLOT-UPDATE-ARM-COMPILED | **OPEN, sequencing** | The family is 2 of 60 unsupported fixtures. Lifting it needs the `finalize` trigger seam in `analyze.pl`, and the delete half additionally needs the edge-negation seam (6 more fixtures). Not this lab's work |

## Fixture candidates for conformance promotion

Distilled `fixture/5` terms, ready to lift into
`v6/prolog/conformance/fixtures/`. None is promoted by this lab; they are the
recoverable output.

1. `update_arm_yields_old_and_new`: U1's program and schedule, expectations
   `final(changed_value/3, [changed_value(cli,v1,v2)])` plus the four-tick
   delta list. Pins the +1 placement, which no current fixture pins.
2. `update_arm_collapses_same_tick_replaces_to_endpoints`: U4 verbatim. Pins
   the ruled collapse against the two phantom readings and the two-row reading.
3. `update_arm_over_a_level_view_needs_no_key`: U1c. Pins that the arm is a
   property of the minus delta, not of `keyed/2`.
4. `delete_arm_separates_from_update_by_negation`: U3b, both schedules. Pins
   the zero-construct answer to SLOT-CAUSE's first half.
5. `match_block_update_arm_matches_hand_written`: U6's byte-identity check,
   the same shape as the landed
   `match_classify_response` / `match_classify_response_desugared` pair.

Fixtures 1-4 would land in the UNSUPPORTED sweep bucket on arrival
(`edge_body_needs_finalize`, and 4 also `edge_body_needs_negation`), which is
the existing state of the two departure fixtures already in the corpus.

## Grades

Lab, run twice, exit 0 both times, PASS-only stdout:

```
$ cd v6/prolog && swipl -q -l labs/update_arm/lab.pl -g go -g halt
PASS  u0_literal_level_rule_spelling_is_refused
PASS  u0_edge_rule_spelling_loads
PASS  u1_keyed_replace_yields_one_pair
PASS  u1_arm_fires_the_tick_after_the_replace
PASS  u1c_the_arm_works_over_a_derived_level_view
PASS  u2_plain_insert_leaves_the_arm_silent
PASS  u3_plain_delete_leaves_the_arm_silent
PASS  u3b_delete_is_separable_by_negating_the_current_side
PASS  u3b_replace_does_not_trip_the_delete_arm
PASS  u4_same_tick_double_replace_yields_the_endpoint_pair
PASS  u4b_same_tick_replaces_from_empty_yield_nothing
PASS  u5_log_rel_arm_is_silently_dead
PASS  u5b_retention_prune_emits_no_departure
PASS  u6_match_block_composes_into_the_update_arm
PASS  u6_match_block_tick_log_matches_the_hand_written_rule
PASS  u6b_finalize_in_a_level_match_arm_is_refused
PASS  s1_arm_reads_a_trigger_column
PASS  s2_sibling_binding_in_a_head_column_throws
PASS  s3_sibling_binding_in_a_body_atom_silently_widens
exit=0
```

Sabotage probes (scratch, not committed), all red as required:

```
red-as-required  u4_phantom_pair_v1_v2        got [changed_value(cli,v1,v3)]
red-as-required  u4_two_rows                  got [changed_value(cli,v1,v3)]
red-as-required  u1_same_tick_placement       got [[],[],[+changed_value(cli,v1,v2)],[]]
red-as-required  s3_shared_scope_one_row      got [echoed(api,api),echoed(api,worker)]
red-as-required  u3_delete_produces_a_row     got []
red-as-required  u5_log_arm_produces_a_row    got []
```

No-drift:

```
$ cd v6/prolog && swipl -q -l conformance/go.pl -g go -g halt
126 PASS / 0 fail

$ bash v6/prolog/compile/scripts/roundtrip.sh
G1: ALL PASS
G2: NO PARSE ERRORS
conformance: 126 pass / 0 fail
roundtrip.sh: ALL GRADES PASS
exit=0

$ cd v6/tsv2 && bash scripts/sweep.sh          # documentation only
SWEEP total=126 compiled=66 unsupported=60 crash=0     # stage 1, unchanged
stage 2 oracle dump: ORACLE_OK on every fixture
stage 3: N/A-with-reason (no node_modules in a fresh worktree, no lockfile)
```

Worktree is clean at the end of the lab: `git status --porcelain` empty.

## Lab death

`v6/prolog/labs/update_arm/` stays alive on this branch so the coordinator can
re-run it during review, and dies in the coordinator's post-merge lab-death
commit per the lab protocol. Last full copy of the lab: **`be019a99`**
(`git show be019a99:v6/prolog/labs/update_arm/lab.pl` recovers it). This
verdict plus the five fixture candidates above are the durable output.
