# REPORT: edge arm order blast radius

base sha: 80ba9db6

## Q1. Where the shape already exists

Programs with TWO OR MORE edge rules (`<+`) heading the SAME relation. Per conformance
fixture (`.pl`), per `.dl6` program, and per rxoracle case program.

### merge_family.pl

| fixture (file:line) | head/arity | head kind | arms (file:line) | trigger of each arm |
|---|---|---|---|---|
| merge_batches_per_tick (:15) | out/1 | log | merge_family.pl:19, :20 | event_a, event_b |
| merge_never_retracts (:31) | out/1 | log | :35, :36 | event_a, event_b |
| key_last_write_wins (:43) | latest/2 | keyed | :47, :48 | from_poll, from_push |
| key_identical_write_is_silent (:58) | latest/2 | keyed | :62, :63 | from_poll, from_push |
| key_same_tick_ordered_not_conflict (:74) | latest/2 | keyed | :78, :79 | from_poll, from_push |
| counter_fold_matches_hand_computation (:87) | counter/2 | keyed | :91, :92 | increment, decrement |
| seed_and_transition_are_disjoint (:108) | counter/2 | keyed | :111, :112 | increment, increment |
| increment_decrement_same_tick_nets_zero (:138) | counter/2 | keyed | :142, :143 | increment, decrement |
| one_occurrence_two_rows_still_conflicts (:151) | latest/2 | keyed | :154, :155 | ping, ping |

### state_machine.pl

| fixture (file:line) | head/arity | head kind | arms (file:line) | trigger of each arm |
|---|---|---|---|---|
| async_state_machine_with_pattern_scan (:23) | phase/2 | keyed | :29, :32, :34, :36 | poll_due; fetch_result(fresh); fetch_result(unchanged); fetch_result(error) |
| async_state_machine_with_pattern_scan (:23) | retries/2 | keyed | :42, :45 | fetch_result(error), fetch_result(fresh) |
| same_tick_error_then_fresh_chains_arms (:79) | retries/2 | keyed | :83, :86 | fetch_result(error), fetch_result(fresh) |

### scopes.pl

| fixture (file:line) | head/arity | head kind | arms (file:line) | trigger of each arm |
|---|---|---|---|---|
| concat_program_queue (:147) | open_tab/2 | keyed | :155, :165 | open_request, finalize(live_tab) |
| take_until_keyed_replace_negated_done (:273) | phase/2 | keyed | :278, :279 | poll_due, fetch_result |
| state_flap_nets_to_zero_scope_churn (:306) | phase/2 | keyed | :311, :312 | poll_due, fetch_result |

### seq_wire.pl

| fixture (file:line) | head/arity | head kind | arms (file:line) | trigger of each arm |
|---|---|---|---|---|
| seq_wire_hand (:24) | seq_numbered_1/2 | keyed | :33, :35 | arrival, arrival |
| seq_wire_hand (:24) | numbered/2 | log | :38, :40 | arrival, arrival |

### engine_core.pl

| fixture (file:line) | head/arity | head kind | arms (file:line) | trigger of each arm |
|---|---|---|---|---|
| created_at_pinned_updated_at_advances (:108) | thing/4 | keyed | :111, :114 | arrive, arrive |

### operators.pl

| fixture (file:line) | head/arity | head kind | arms (file:line) | trigger of each arm |
|---|---|---|---|---|
| repeat_is_a_self_carry_chain (:24) | pulse/1 | log | :27, :28 | kick, pulse |

### 8_json_flex.pl

| fixture (file:line) | head/arity | head kind | arms (file:line) | trigger of each arm |
|---|---|---|---|---|
| json_typed_capture_folds_into_a_keyed_int_total (:294) | total/2 | keyed | :302, :303 | star_event, star_event |

### golden-flex.dl6 (whole file is one program)

| head/arity | head kind | arms (file:line) | trigger of each arm |
|---|---|---|---|
| pick_count/2 | keyed (`key(1)`, :261) | :262, :264 | pick_event, pick_event |

### rxoracle case program: scan_state_feedback/leg-b.dl6

| head/arity | head kind | arms (file:line) | trigger of each arm |
|---|---|---|---|
| counter/2 | keyed (`key(1)`) | :24, :25 | increment, increment |

### Zero hits

Fixtures/programs examined with `<+` that have NO two arms on one head within a single
program: check_eventing.pl, spine_semantics.pl, temporal_pipe.pl, shell_stream.pl,
scopes.pl (all other fixtures), body_words.pl, 7_coalesce.pl, 1_match_block.pl,
6_relation_depth.pl, and all other `.dl6` fixtures except golden-flex.dl6. Command:
`grep -rn '<+'`, then per-file read of the declaring fixture boundaries.

## Q2. Order-sensitive in FINAL STATE

Of the Q1 rows whose head is a LOG rel, which declare `keep(count(N))` (order decides
which row survives) vs `keep(all)` (order only shows in the delta sequence):

| Q1 row | head/arity | keep decl (file:line) | final-state order role |
|---|---|---|---|
| merge_batches_per_tick | out/1 | keep(out/1, all), merge_family.pl:18 | order shows only in delta sequence |
| merge_never_retracts | out/1 | keep(out/1, all), merge_family.pl:34 | order shows only in delta sequence |
| seq_wire_hand | numbered/2 | keep(numbered/2, all), seq_wire.pl:29 | order shows only in delta sequence |
| repeat_is_a_self_carry_chain | pulse/1 | keep(pulse/1, all), operators.pl:26 | order shows only in delta sequence |

Zero Q1 rows use `keep(count(N))`. Every checked-in log same-head arm is `keep(all)`, so
no existing fixture has the measured "order decides which row survives retention"
shape. The `keep(count(N))` fixtures that DO exist (engine_core.pl:17 retention_
count_prunes_oldest, :43 retention_prune_is_a_visible_minus, :72 finalize_over_log_
fires_on_retention_prune) have no same-head edge arms under the counted head.
Command: `grep -rn 'keep(count' v6/prolog/conformance/fixtures/ v6/dl/fixtures/*.dl6`
(3 hits, all engine_core.pl + a golden-flex comment at :250).

## Q3. Consumers of rule-list ORDER

| file:line | predicate | what order decides |
|---|---|---|
| engine.pl:355-360 | process_occurrences/7 | findall over Rules builds `Edges` in program order; ORDER DECIDES OUTPUT (arm firing order) |
| engine.pl:378-385 | process_occurrences_/5 | findall over Edges + dedupe_keep_order gives derived-head list in arm order; ORDER DECIDES OUTPUT (write order, seq assignment) |
| engine.pl:391-397 | check_occurrence_conflicts/2 | forall/findall over Derived; order is merely traversal (throws keyed_conflict on two rows per key) |
| engine.pl:399-417 | apply_edge_writes/5 | appends log rows / replaces keyed rows in Derived order; next_seq assigns increasing Seq per row, so arm order decides survival under count pruning; ORDER DECIDES OUTPUT |
| compile.pl:173-175 | program_plan | EdgeRules = include(rule_is_edge, Rules, ...), program (source) order; source of the order downstream |
| lower.pl:210-223 | statement_rule_ids/3 | ordinal `#N` per head by order of the HeadRefs list; ORDER DECIDES OUTPUT (ruleId bytes) |
| lower.pl:3392-3413 | lower_program/6 | EdgeStatements = maplist over EdgeRules + append, order preserved; ORDER DECIDES OUTPUT (statement list + ruleId ordinals) |
| emit_ts.pl:862-870 | incremental_edge_statement_lines/4 | renders INCREMENTAL_EDGE_STATEMENTS array and ruleIds in EdgeStatements order; ORDER DECIDES OUTPUT (bytes) |
| emit_ts.pl:1781, 1885 | (render sites) | array/field references; order is merely traversal |
| analyze.pl:1331-1350 | check_no_edge_head_conflict_risk/2 | nth0 pairwise trigger comparison; order is merely traversal (pair enumeration); only checks KEYED heads, so a log head is not inspected |
| 3_clock_check.pl:44 | clock_dependencies/2 | nth1/3 indexes rules into RuleId (rule(Index,...)) inside dependency facts; order is merely traversal / an identifier, not execution order |

## Q4. Artifacts whose bytes depend on arm order

| artifact | count | order-bearing content |
|---|---|---|
| v6/tsv2/gen_emitted/*.ts | 197 files, 388 `ruleId:` fields | `#N` ordinal + INCREMENTAL_EDGE_STATEMENTS array order. 23 `#2` ruleIds across 21 files (multi-arm same head). Example: merge_batches_per_tick.ts:253 (`out/1#1`), :254 (`out/1#2`) |
| v6/prolog/compile/out/*.ts | 196 files, 386 `ruleId:` fields | same content/ruleIds as gen_emitted (196 of the 197; door-handwritten.ts has no compile/out twin). Example: compile/out/merge_batches_per_tick.ts:253-254 |
| v6/tsv2/goldens/trace-line.jsonl | 1 file | embeds ruleId ordinals per tick, e.g. line 1 `"rule":"<program>:merged/1#1"`, `"#2"` |
| v6/prolog/compile/out/manifest.json | 283 lines (~280 entries), 0 `ruleId:` | no arm order; carries refusal reason strings (order-independent), e.g. :211 `edge_head_conflict_risk(latest/2,[ping/1])` |
| v6/prolog/compile/out/*.schedule.json, *.oracle.jsonl | 1 each per fixture | no ruleId; log delta sequences could shift for a log head but carry no rule ordinal |

## Q5. Existing refusal / violation names (edge, keyed, conflict, retention)

| name | file:line |
|---|---|
| keyed_conflict/3 | engine.pl:397 |
| edge_into_unkeyed_set/1 | engine.pl:415 |
| retract_from_log/1 | engine.pl:331 |
| edge_head_conflict_risk/2 | analyze.pl:1350 |
| keyed_level_head/1 | analyze.pl:1219, engine.pl:203, 3_clock_check.pl:311 |
| keyed_log_rel/1 | engine.pl:204 |
| keyed_log_rel/2 | analyze.pl:1222 |
| missing_retention/1 | analyze.pl:1228, engine.pl:206 |
| keep_on_non_log_rel/1 | analyze.pl:1224, engine.pl:207 |
| log_on_level_headed_rel/1 | analyze.pl:1223, engine.pl:205, 3_clock_check.pl:306 |
| aggregate_in_edge_head | analyze.pl:1232, engine.pl:209 |
| aggregate_head_shape | analyze.pl:1233, engine.pl:210 |
| aggregate_not_implemented | analyze.pl:1237, engine.pl:211 |
| aggregate_head_reads_itself/1 | analyze.pl:1499 |
| aggregate_head_no_positive_body/1 | analyze.pl:1503 |
| aggregate_operand_not_number/4 | engine.pl:219 |
| edge_body_with_latest | analyze.pl:929 |
| edge_body_with_finalize | analyze.pl:938 |
| edge_body_with_now | analyze.pl:953 |
| edge_body_with_negation | analyze.pl:961 |
| edge_body_needs_json_destructure | analyze.pl:965 |
| edge_body_multiple_finalize | analyze.pl:850 |
| edge_head_column_type_mismatch | analyze.pl:1036 |
| head_arithmetic | analyze.pl:1284 |
| compound_pattern_on_arrival_rel/3 | analyze.pl:1426 |
| level_body_goal/2 | analyze.pl:1449 |
| now_in_level_rule/2 | analyze.pl:1460 |
| negated_guard_goal/2 | analyze.pl:1463 |
| column_ref_type_conflict/2 | analyze.pl:771 |
| clock_path_conflict/4 | 3_clock_check.pl:328 (refusal_reason :395) |
| unconstructive_clock_cycle/2 | 3_clock_check.pl:340 (refusal_reason :396) |
| type_cycle/1 | engine.pl:176, analyze.pl:1187 |
| relation_pattern_not_a_relation_value/4 | engine.pl:177, analyze.pl:1188 |
| dynamic_relation_name/1 | engine.pl:180, analyze.pl:1191 |
| reserved_body_word/1 | engine.pl:182, analyze.pl:1196 |
| relation_value_under_negation/4 | engine.pl:183, analyze.pl:1198 |
| relation_value_in_edge_rule/4 | engine.pl:186, analyze.pl:1201 |
| relation_column_type_conflict/6 | engine.pl:189, analyze.pl:1204 |
| head_column_type_conflict/6 | engine.pl:195, analyze.pl:1211 |
| column_type_unknown/1 | engine.pl:200, analyze.pl:1216 |
| key_position_out_of_range | engine.pl:201, analyze.pl:1217 |
| key_position_duplicate | engine.pl:202, analyze.pl:1218 |
| finalize_in_level_rule/1 | engine.pl:222, analyze.pl:1225, 3_clock_check.pl:298 |
| latest_in_level_rule/1 | engine.pl:223, analyze.pl:1226, 3_clock_check.pl:316 |
| pre_in_level_rule/1 | engine.pl:224, analyze.pl:1227, 3_clock_check.pl:302 |

Note: every same-area name (keyed_conflict, edge_into_unkeyed_set, edge_head_conflict_
risk, keyed_level_head, keyed_log_rel) concerns a KEYED head or an unkeyed set; none
covers "two edge arms on one LOG head where retention makes order decide the survivor",
which is the measured un-refused shape.

## Q6. clock_violation/2 clauses

All clauses in 3_clock_check.pl, 5-word summaries:

| file:line | clause head | refuses |
|---|---|---|
| 3_clock_check.pl:298 | cross_plane(finalize_in_level_rule(Ref)) | finalize in a level body |
| 3_clock_check.pl:302 | cross_plane(pre_in_level_rule(Ref)) | pre in a level body |
| 3_clock_check.pl:306 | cross_plane(log_on_level_headed_rel(Ref)) | log kind on a level head |
| 3_clock_check.pl:311 | cross_plane(keyed_level_head(Ref)) | keyed on a level head |
| 3_clock_check.pl:316 | cross_plane(latest_in_level_rule(Ref)) | latest in a level body |
| 3_clock_check.pl:328 | clock_path_conflict(Origin, Ref, Left, Right) | a rel reachable at two offsets |
| 3_clock_check.pl:340 | unconstructive_clock_cycle(Component, Reason) | nonpositive / nonconstructive clock cycle |

## Deviations

- The brief's example command `grep -c 'ruleId:' v6/tsv2/gen_emitted/golden-flex.ts`
  fails: no `golden-flex.ts` exists under `v6/tsv2/gen_emitted/`. golden-flex.dl6 (the
  fixture with the same-head `pick_count/2` edge arms) is not compiled to any
  checked-in generated `.ts`; `gen_emitted/` and `compile/out/` hold only fixture
  (`.pl`)-derived files.
- Everything else matched: base sha is 80ba9db6; engine.pl:397 throws keyed_conflict/3,
  engine.pl:415 throws edge_into_unkeyed_set, and apply_edge_writes writes log heads by
  append in derived order.
