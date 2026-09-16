# REPORT: edge arm order blast radius
base sha: 80ba9db6

Method notes. Q1 found same-head multi-arm edge programs by parsing each
program's rule list and grouping edge heads by functor/arity (a read-only
scan, `python3 /tmp/parse_fixtures3.py`, run over the conformance fixtures).
The `.dl6` catalog at `v6/prolog/compile/dl_view/` is the dl6 surface spelling
of the same conformance programs, listed under "other program text" with one
row per program. Declarations (`log`/`keyed`) come from each program's own
`kind(...)`/`keyed(...)`/`rel ... log`/`rel ... key(...)` lines, read directly.

## Q1: where the shared-head shape already exists

### Conformance fixtures, `v6/prolog/conformance/fixtures/*.pl`

| file:line | fixture | head | trigger of each arm | head decl |
|---|---|---|---|---|
| merge_family.pl:15(arms 19,20) | merge_batches_per_tick | out/1 | event_a; event_b | log keep(all) |
| merge_family.pl:31(arms 35,36) | merge_never_retracts | out/1 | event_a; event_b | log keep(all) |
| merge_family.pl:43(arms 47,48) | key_last_write_wins | latest/2 | from_poll; from_push | keyed |
| merge_family.pl:58(arms 62,63) | key_identical_write_is_silent | latest/2 | from_poll; from_push | keyed |
| merge_family.pl:74(arms 78,79) | key_same_tick_ordered_not_conflict | latest/2 | from_poll; from_push | keyed |
| merge_family.pl:87(arms 91,92) | counter_fold_matches_hand_computation | counter/2 | increment; decrement | keyed |
| merge_family.pl:108(arms 111,112) | seed_and_transition_are_disjoint | counter/2 | increment; increment | keyed |
| merge_family.pl:138(arms 142,143) | increment_decrement_same_tick_nets_zero | counter/2 | increment; decrement | keyed |
| merge_family.pl:151(arms 154,155) | one_occurrence_two_rows_still_conflicts | latest/2 | ping; ping | keyed |
| operators.pl:24(arms 27,28) | repeat_is_a_self_carry_chain | pulse/1 | kick; pulse | log keep(all) |
| seq_wire.pl:24(arms 33,35) | seq_wire_hand | seq_numbered_1/2 | arrival; arrival | keyed |
| seq_wire.pl:24(arms 38,40) | seq_wire_hand | numbered/2 | arrival; arrival | log keep(all) |
| state_machine.pl:23(arms 29,32,34,36) | async_state_machine_with_pattern_scan | phase/2 x4 | poll_due; fetch_result; fetch_result; fetch_result | keyed |
| state_machine.pl:23(arms 42,45) | async_state_machine_with_pattern_scan | retries/2 | fetch_result; fetch_result | keyed |
| state_machine.pl:79(arms 83,86) | same_tick_error_then_fresh_chains_arms | retries/2 | fetch_result; fetch_result | keyed |
| engine_core.pl:108(arms 111,114) | created_at_pinned_updated_at_advances | thing/4 | arrive; arrive | keyed |
| 8_json_flex.pl:294(arms 302,303) | json_typed_capture_folds_into_a_keyed_int_total | total/2 | star_event; star_event | keyed |
| scopes.pl:147(arms 155,165) | concat_program_queue | open_tab/2 | open_request; finalize(live_tab) | keyed |
| scopes.pl:273(arms 278,279) | take_until_keyed_replace_negated_done | phase/2 | poll_due; fetch_result | keyed |
| scopes.pl:306(arms 311,312) | state_flap_nets_to_zero_scope_churn | phase/2 | poll_due; fetch_result | keyed |

### dl6 fixtures, `v6/dl/fixtures/`

| file:line | head | trigger of each arm | head decl |
|---|---|---|---|
| golden-flex.dl6:262,264 | pick_count/2 | pick_event; pick_event | key(1) |

### Other program text found

`v6/prolog/compile/dl_view/*.dl6` (dl6 surface mirrors of the conformance
fixtures; each duplicates a fixture above):

| file:line | head | trigger | decl |
|---|---|---|---|
| dl_view/merge_batches_per_tick.dl6:5,6 | out/1 | event_a; event_b | log keep(all) |
| dl_view/merge_never_retracts.dl6:5,6 | out/1 | event_a; event_b | log keep(all) |
| dl_view/key_identical_write_is_silent.dl6:5,6 | latest/2 | from_poll; from_push | key(1) |
| dl_view/key_last_write_wins.dl6:5,6 | latest/2 | from_poll; from_push | key(1) |
| dl_view/key_same_tick_ordered_not_conflict.dl6:5,6 | latest/2 | from_poll; from_push | key(1) |
| dl_view/one_occurrence_two_rows_still_conflicts.dl6:4,5 | latest/2 | ping; ping | key(1) |
| dl_view/counter_fold_matches_hand_computation.dl6:5,6 | counter/2 | increment; decrement | key(1) |
| dl_view/seed_and_transition_are_disjoint.dl6:4,5 | counter/2 | increment; increment | key(1) |
| dl_view/increment_decrement_same_tick_nets_zero.dl6:5,6 | counter/2 | increment; decrement | key(1) |
| dl_view/repeat_is_a_self_carry_chain.dl6:4,5 | pulse/1 | kick; pulse | log keep(all) |
| dl_view/seq_wire_hand.dl6:5,6 | seq_numbered_1/2 | arrival; arrival | key(1) |
| dl_view/seq_wire_hand.dl6:7,8 | numbered/2 | arrival; arrival | log keep(all) |
| dl_view/async_state_machine_with_pattern_scan.dl6:7,8,9,10 | phase/2 x4 | poll_due; fetch_result x3 | key(1) |
| dl_view/async_state_machine_with_pattern_scan.dl6:12,13 | retries/2 | fetch_result; fetch_result | key(1) |
| dl_view/same_tick_error_then_fresh_chains_arms.dl6:5,6 | retries/2 | fetch_result; fetch_result | key(1) |
| dl_view/created_at_pinned_updated_at_advances.dl6:4,5 | thing/4 | arrive; arrive | key(1) |
| dl_view/take_until_keyed_replace_negated_done.dl6:6,7 | phase/2 | poll_due; fetch_result | key(1) |
| dl_view/state_flap_nets_to_zero_scope_churn.dl6:6,7 | phase/2 | poll_due; fetch_result | key(1) |
| dl_view/concat_program_queue.dl6:10,18 | open_tab/2 | open_request; finalize(live_tab) | key(1) |

`v6/prolog/labs/**` (seed/step fold pairs, mostly same head):

| file:line | head | trigger | decl |
|---|---|---|---|
| labs/rel_as_stream/ordinal_stream.dl6:8,9 | stream/3 | event; event | log keep(all) |
| labs/rel_as_stream/ordinal_stream.dl6:6,7 | cursor/2 | event; event | keyed |
| labs/rel_as_stream/backpressure.dl6:14,15 | chan/2 | produce; produce | log keep(all) |
| labs/rel_as_stream/backpressure.dl6:11,12 | cursor/2 | produce; produce | keyed |
| labs/csp_idioms/rendezvous.dl6:28,30 | pend_offer/2 | offer; offer | log keep(all) |
| labs/csp_idioms/rendezvous.dl6:27,29 / 33,35 | cursor_o/2, cursor_r/2 | offer/arrive | keyed |
| labs/csp_idioms/rendezvous.dl6:34,36 | pend_recv/2 | arrive; arrive | log keep(all) |
| labs/csp_idioms/workerpool.dl6:19,21 | pending/2 | produce; produce | log keep(all) |
| labs/csp_idioms/workerpool.dl6:18,20 | cursor/2 | produce; produce | keyed |
| labs/csp_idioms/fanout.dl6:22,24 | pending/2 | produce; produce | log keep(all) |
| labs/csp_idioms/fanout.dl6:23,21 / 26,33 | cursor/2, read_at/2 | produce/attach,poll | keyed |
| labs/csp_idioms/select.dl6:29,31 / 35,37 | pend_a/2, pend_b/2 | prod_a/prod_b | log keep(all) |
| labs/csp_idioms/fanin.dl6:22,24 | pending/2 | merged; merged | log keep(all) |
| labs/csp_idioms/fanin.dl6:21,23 | cursor/2 | merged; merged | keyed |
| labs/csp_idioms/buffered.dl6:17,19 | pending/2 | produce; produce | log keep(all) |
| labs/csp_idioms/buffered.dl6:18,16 | cursor/2 | produce | keyed |
| labs/csp_idioms/done.dl6:22,24 | pending/2 | produce; produce | log keep(all) |
| labs/csp_idioms/done.dl6:23,21 | cursor/2 | produce | keyed |
| labs/csp_idioms/semaphore.dl6:23,24 | granted/2 | acquire; acquire | log keep(all) |
| labs/csp_idioms/semaphore_naive.dl6:24 (single arm) | granted/2 | acquire | log keep(all) |
| v6/tsv2/rxoracle/cases/scan_state_feedback/leg-b.dl6:24,25 | counter/2 | increment; increment | keyed |

## Q2: order-sensitive in final state?

Definition from the brief: head is `log`; `keep(count(N))` means arm order
decides which row survives retention; `keep(all)` means order only shows in the
delta sequence. Of the Q1 rows whose head is `log`:

| file:line | head | keep decl | order decides |
|---|---|---|---|
| merge_family.pl:18 | out/1 | keep(out/1, all) | delta sequence only |
| merge_family.pl:34 | out/1 | keep(out/1, all) | delta sequence only |
| operators.pl:26 | pulse/1 | keep(pulse/1, all) | delta sequence only |
| seq_wire.pl:29 | numbered/2 | keep(numbered/2, all) | delta sequence only |
| dl_view/merge_batches_per_tick.dl6:3 | out/1 | keep(all) | delta sequence only |
| dl_view/merge_never_retracts.dl6:3 | out/1 | keep(all) | delta sequence only |
| dl_view/repeat_is_a_self_carry_chain.dl6:3 | pulse/1 | keep(all) | delta sequence only |
| dl_view/seq_wire_hand.dl6:3 | numbered/2 | keep(all) | delta sequence only |
| labs ordinals/csp pending etc. | stream/3, pend_* /2, pending/2, chan/2, granted/2 | keep(all) | delta sequence only |

`keep(count(N))` with two or more edge arms on the same log head: zero hits.
The only `keep(count(N))` log rels are `event/1` (engine_core.pl:17,43, rule
list empty), `ev/2` count(2) (engine_core.pl:72, single arm `gone/2`),
`recent_pick` count(2) (golden-flex.dl6:250, single source), `ev/2`
count(2) (retention_event.dl6:4), `chan/3` count(3) (buffered.dl6:24). None
has more than one arm on that head. Command: `grep -rn 'keep(count' v6 -r`;
then per match, count `<+` arms on the same head from the reads above.

The exact shape in the brief's measured example (`kind(journal/1,log),
keep(journal/1,count(1))` plus two `journal(_) <+ ping(_)` arms) does not
exist as any program in the worktree.

## Q3: who consumes rule-list order

| file:line | predicate / site | what order decides |
|---|---|---|
| conformance/engine.pl:355-360 | process_occurrences/7 | builds `Edges` by `member((Head<+Body),Rules)`, so text order fixes edge iteration order. Decides output. |
| conformance/engine.pl:378-385 | process_occurrences_/6 | `findall(EvaluatedHead, member(Edge,Edges)...)` runs each occurrence against arms in `Edges` order; `dedupe_keep_order` keeps that order. Decides output. |
| conformance/engine.pl:391-397 | check_occurrence_conflicts/2 | pairwise `keyed_conflict/3` throw; pairwise so order-independent. Merely traversal. |
| conformance/engine.pl:399-417 | apply_edge_writes/6 | walks `Derived` in order; log head appends `lrow(st(Tick,Seq),Row)` with monotonically increasing Seq (next_seq, 419-421), so arm order sets log sequence; keyed head replaces so later arm wins. Decides output. |
| lower.pl:77 | comment on apply_edge_writes/6 | "across occurrences the later write wins", the same order rule for arms. |
| lower.pl:210-223 | statement_rule_ids/3 | numbers same-head arms `1,2,...` via statement_ordinals; comment 206-208 says the id "moves when two arms of one head are reordered". Decides output (the ruleId string). |
| emit_ts.pl:862-870 | incremental_edge_statement_lines/4 | calls statement_rule_ids/3 (864) and emits `INCREMENTAL_EDGE_STATEMENTS` array in `EdgeStatements` order. Decides output. |
| emit_ts.pl:874-887 | incremental_edge_statement_entry_line/6 | renders each arm's `ruleId` from that ordinal. Decides output. |
| emit_ts.pl:889-898 | incremental_level_statement_lines/4 | same numbering for level statements (891). Decides output. |
| emit_ts.pl:1885 | applyEdges runtime call | `concatMap(() => ...applyEdges(seam, INCREMENTAL_EDGE_STATEMENTS,...))`; array order fixes per-tick write order. Decides output. |
| analyze.pl:1338-1350 | check_no_edge_head_conflict_risk/2 | `member(Rule,EdgeRules)`, pairwise `IndexA<IndexB` intersection; check itself order-independent. Merely traversal. |
| analyze.pl:1321-1322 | comment | acknowledges lowering "lets the last-running arm's write win" when no conflict check fires. Decides output (documented). |
| 3_clock_check.pl:41-49 | clock_dependencies/2 | `nth1(Index,Rules,Rule)` indexes each rule; Index feeds rule_dependencies as rule identity; final list is `sort/2` (line 49). Merely traversal (sorted output). |

## Q4: artifacts whose bytes change if two arms are reordered

| artifact class | location | depends on arm order | example file:line |
|---|---|---|---|
| emitted TS, edge statement plans | v6/tsv2/gen_emitted/*.ts | yes, `ruleId` holds arm ordinal and array entry order | merge_batches_per_tick.ts:253 (`out/1#1`), :254 (`out/1#2`) |
| emitted TS, same plans (compiler output) | v6/prolog/compile/out/*.ts | yes | merge_batches_per_tick.ts:253-254 |
| oracle delta sequence | v6/prolog/compile/out/*.oracle.jsonl | yes, `add` order mirrors arm order for a log head | merge_batches_per_tick.oracle.jsonl:2 (`"out":{"add":[["beta"],["gamma"]]}`) |
| oracle final rows | v6/prolog/compile/out/*.oracle.final.jsonl | yes for a log `keep(count(N))` head (none ship), no for `keep(all)` | n/a, see Q2 |
| schedule files | v6/prolog/compile/out/*.schedule.json | no, arrivals only | n/a |
| manifest | v6/prolog/compile/out/manifest.json | no, fixture inventory of name/file/bucket/reason, zero `ruleId` fields (count 0) | manifest.json:2 |
| golden expected ticks | v6/tsv2/goldens/ghcacher_tick_golden/2_expected.tick.jsonl, 3_expected.final.jsonl | yes, any delta whose program has shared-head arms | n/a (ghcacher program has no shared-head log arm) |

Counts: v6/tsv2/gen_emitted has 197 `.ts`, 179 contain `ruleId`
(`grep -c 'ruleId:' v6/tsv2/gen_emitted/golden-flex.ts` returned 0 because
that file is the from-scratch dl6 fixture's emitted module and uses the
brace-less `headRel` entry pattern; the per-program sweep files carry the
`ruleId:` form). v6/prolog/compile/out has 178 `.ts` with `ruleId`.
manifest.json has 0 `ruleId` occurrences.

## Q5: existing refusal / violation names in this area

Engine-door refusals (`engine_refusal/3`, conformance/engine.pl):

| file:line | name |
|---|---|
| engine.pl:201 | key_position_out_of_range |
| engine.pl:202 | key_position_duplicate |
| engine.pl:203 | keyed_level_head |
| engine.pl:204 | keyed_log_rel |
| engine.pl:205 | log_on_level_headed_rel |
| engine.pl:206 | missing_retention |
| engine.pl:207 | keep_on_non_log_rel |
| engine.pl:209 | aggregate_in_edge_head |
| engine.pl:222 | finalize_in_level_rule |
| engine.pl:223 | latest_in_level_rule |
| engine.pl:224 | pre_in_level_rule |

Run-time throws in the engine:

| file:line | term |
|---|---|
| engine.pl:397 | keyed_conflict/3 |
| engine.pl:415 | edge_into_unkeyed_set/1 |
| engine.pl:126 | missing_retention (load error) |

Compiler refusals (`unsupported_construct/1`):

| file:line | name |
|---|---|
| lower.pl:1578 | edge_into_unkeyed_set(HeadRef) |
| lower.pl:1510 | edge_trigger_not_log(TriggerRef) |
| analyze.pl:1350 | edge_head_conflict_risk(HeadRef, Shared) |
| analyze.pl:1036 | edge_head_column_type_mismatch(...) |
| analyze.pl:850 | edge_body_multiple_finalize(Body) |
| analyze.pl:771 | column_ref_type_conflict(Left,Right) |
| analyze.pl:1228 | missing_retention(Ref), compiler door |

Clock violations and their packaging:

| file:line | package / reason |
|---|---|
| 3_clock_check.pl:298 | clock_violation cross_plane(finalize_in_level_rule(Ref)) |
| 3_clock_check.pl:302 | clock_violation cross_plane(pre_in_level_rule(Ref)) |
| 3_clock_check.pl:306 | clock_violation cross_plane(log_on_level_headed_rel(Ref)) |
| 3_clock_check.pl:311 | clock_violation cross_plane(keyed_level_head(Ref)) |
| 3_clock_check.pl:316 | clock_violation cross_plane(latest_in_level_rule(Ref)) |
| 3_clock_check.pl:328 | clock_violation clock_path_conflict(Origin,Ref,Left,Right) |
| 3_clock_check.pl:340 | clock_violation unconstructive_clock_cycle(Component,Reason) |
| 3_clock_check.pl:20 | clock_refusal_reason/1 (imported into 0_refusal_messages.pl:20) |

0_refusal_messages.pl derives its `refusal_inventory_name/1` set from registry
rows and loaded `unsupported_construct/1` clauses (0_refusal_messages.pl:122-130);
it holds no separate hardcoded name list to collide with. The reason NAME comes
from the functor of the payload term (0_refusal_messages.pl:116-120).

## Q6: the shape of `clock_violation/2` (list only, no judgment)

| file:line | reason | 5-word refusal summary |
|---|---|---|
| 3_clock_check.pl:298 | cross_plane(finalize_in_level_rule(Ref)) | level body finalize, refuses. |
| 3_clock_check.pl:302 | cross_plane(pre_in_level_rule(Ref)) | level rule pre, refuses. |
| 3_clock_check.pl:306 | cross_plane(log_on_level_headed_rel(Ref)) | log rel level-headed, refuses. |
| 3_clock_check.pl:311 | cross_plane(keyed_level_head(Ref)) | a keyed rel level-headed. |
| 3_clock_check.pl:316 | cross_plane(latest_in_level_rule(Ref)) | level rule latest, refuses. |
| 3_clock_check.pl:328 | clock_path_conflict(Origin,Ref,Left,Right) | clock path offsets conflict, refuses. |
| 3_clock_check.pl:340 | unconstructive_clock_cycle(Component,Reason) | cyclic clock, unconstructive, refuses. |

## Deviations

- The brief cites `analyze.pl` `check_no_edge_head_conflict_risk/2` "near line
  1332"; the clause head is at analyze.pl:1331.
- The brief's measured example (`kind(journal/1,log)`,
  `keep(journal/1,count(1))` plus two `journal(_) <+ ping(_)` arms) has no
  counterpart program anywhere in the worktree. No log head with two or more
  edge arms declares `keep(count(N))` in any found program.
- The brief's suggested probe `grep -c 'ruleId:' v6/tsv2/gen_emitted/golden-flex.ts`
  returns 0 because that file is the dl6 fixture's emitted module and does not
  use the `ruleId:` string form; the sweep `.ts` files under the same directory
  do (179 of 197).
- A read-only parse of the conformance fixtures is described in Q1 method; no
  files were modified. No test battery, `just`, or build was run.
