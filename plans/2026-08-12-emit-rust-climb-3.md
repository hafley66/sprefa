# emit_rust climb 3

## Context

The lane starts from `e70417d9`, where `v6/sprefa-engine-rs/graded.tsv`
records 392 fixtures and 230 byte-clean tick logs. The remaining executable
rows are 50 diffs: 26 ordered programs, 18 struct-plane programs, and 6
departure-frontier programs. The TypeScript tick phase list is emitted by
`v6/prolog/emit_ts.pl:2518-2571`; the Rust phase list is
`v6/sprefa-engine-rs/src/program.rs:69-110`.

The Rust grader records 106 rows as `unsupported`. Its conjunction spans
planning, lowering, emission, and schedule serialization at
`v6/sprefa-engine-rs/grade.pl:21-44`, so that label alone does not identify an
emitter limitation. The tracked TypeScript manifest classifies 104 of those
same names as `unsupported`. Both emitter predicates were invoked directly for
the other two names and both returned source. Their failure occurs afterward
at `sweep:schedule_json/4`, called by `grade.pl:33`.

## Decisions

| question | selected path | other recorded fork |
|---|---|---|
| First diff cause | Port `StructPlane.intern` and its emitted plan fields. | Ordered programs require new occurrence-arm emitter output. Departure handling covers 6 rows. |
| Unsupported classification | Preserve a third category for the two schedule serialization failures. | Treating every false conjunction as a Rust emitter gap contradicts the direct predicate results. |
| Struct execution order | Follow the TypeScript post-order collection and declared topological type order. | Parent-first execution reads child ids before their target rows. |
| Commits | One measured cause per commit, with the grade transition in the subject. | Combining causes removes the per-cause grade measurement. |

## Unsupported triage

Summary measured before runtime edits:

| classification | count | evidence |
|---|---:|---|
| Accepted by `emit_ts`, missing from `emit_rust` | 0 | Direct emitter comparison plus the current TypeScript manifest. |
| Unbuilt in both | 104 | Each name is `unsupported` in `v6/prolog/compile/out/manifest.json` with the same normalized reason. The shared compiler throw is `v6/prolog/analyze.pl:1287-1291`; direct expansion and lowering throws retain their exact terms. |
| Schedule serialization failure after both emitters succeed | 2 | Both emitters return source. `sweep:schedule_json/4` fails because the expanded `__member/3` relation plan is paired with authored `__member/2` schedule terms. The false conjunction is labeled `emitter returned false` by `v6/sprefa-engine-rs/grade.pl:39-41`. |

The direct refusal sites used by the table include
`v6/prolog/0_coalesce_expand.pl:145-166`,
`v6/prolog/0_generic_expand.pl:49-62`,
`v6/prolog/0_match_expand.pl:111-121`,
`v6/prolog/0_option_expand.pl:28-48`,
`v6/prolog/0_option_expand.pl:99-105`,
`v6/prolog/0_type_plane.pl:123-131`,
`v6/prolog/1_host_expand.pl:139`,
`v6/prolog/1_host_expand.pl:294`,
`v6/prolog/1_host_expand.pl:395`,
`v6/prolog/compile.pl:272`,
`v6/prolog/compile.pl:316-320`,
`v6/prolog/lower.pl:760`,
`v6/prolog/lower.pl:1775`,
`v6/prolog/lower.pl:1826`,
`v6/prolog/lower.pl:2734`,
`v6/prolog/lower.pl:2762`,
`v6/prolog/lower.pl:3006`,
`v6/prolog/lower.pl:3182`, and
`v6/prolog/lower.pl:4934`.

| fixture | classification | measured reason |
|---|---|---|
| `aggregate_in_edge_head_rejected` | unbuilt in both | `aggregate_in_edge_head(total/1)` |
| `arithmetic_rejects_non_int_operand_at_runtime` | unbuilt in both | `arith_operand_not_number(_+1,_,text)` |
| `async_state_machine_with_pattern_scan` | unbuilt in both | `trigger_arg_not_var(fresh(_,_))` |
| `bool_rejects_text_ingress` | unbuilt in both | `type_arrival_shape_mismatch(flag/1,value,bool,field_not_bool(true))` |
| `chain_into_keyed_head_replaces` | unbuilt in both | `edge_body_needs_json_destructure((demand_row(_,_),decode(_,fresh(_,_))))` |
| `coalesce_under_negation_is_refused` | unbuilt in both | `coalesce_not_top_level(latest_commit/2)` |
| `coalesce_with_a_variable_default_is_refused` | unbuilt in both | `coalesce_default_not_literal(latest_commit/2,variable)` |
| `coalesce_with_two_outputs_is_refused` | unbuilt in both | `coalesce_multiple_outputs(commit_by/3,2)` |
| `coalesce_without_an_output_is_refused` | unbuilt in both | `coalesce_no_output(archived/1)` |
| `complete_is_a_named_unsupported` | unbuilt in both | `lifecycle_arm(complete)` |
| `decode_missing_key_fails_quietly` | unbuilt in both | `decode_source_not_struct(decode(_,{absent_key:_}))` |
| `decode_open_pattern_binds_nested` | unbuilt in both | `decode_source_not_struct(decode(_,{name:_}))` |
| `desugared_trace_equals_hand_written` | unbuilt in both | `edge_body_needs_json_destructure((demand_row(_,_),decode(_,fresh(_,_)),stars_of(_,_)))` |
| `duplicate_host_name_is_refused` | unbuilt in both | `duplicate_host_decl(look)` |
| `edge_into_unkeyed_set_rejected` | unbuilt in both | `edge_into_unkeyed_set(sink/1)` |
| `edge_trigger_literal_filters_on_the_oracle_door` | unbuilt in both | `trigger_arg_not_var(200)` |
| `enum_decl_variant_name_collision_is_refused` | unbuilt in both | `enum_variant_name_collision(page)` |
| `enum_variant_field_undeclared_type_still_throws` | unbuilt in both | `column_type_unknown(treee)` |
| `error_is_a_named_unsupported` | unbuilt in both | `lifecycle_arm(error)` |
| `float_rejects_non_float_ingress` | unbuilt in both | `type_arrival_shape_mismatch(score/1,value,float,field_not_finite_float(not_a_number))` |
| `fork_join_error_arm_is_a_value` | unbuilt in both | `compound_pattern_on_arrival_rel(outcome_a/1,1,ok(_))` |
| `ghcacher_host_program_term` | unbuilt in both | `level_body_goal(pull_request(_,_,_,_,_),json_each(_,_))` |
| `ghcacher_json_normalization` | unbuilt in both | `level_body_goal(pull_request(_,_,_,_,_),json_each(_,_))` |
| `guard_stage_fires_on_negation_and_comparison` | unbuilt in both | `edge_body_needs_json_destructure((demand_row(_,_),decode(_,fresh(_,_)),stars_of(_,_),_>100,not(muted(_))))` |
| `guard_stage_silent_below_threshold` | unbuilt in both | `edge_body_needs_json_destructure((demand_row(_,_),decode(_,fresh(_,_)),stars_of(_,_),_>100,not(muted(_))))` |
| `guard_stage_silent_when_muted` | unbuilt in both | `edge_body_needs_json_destructure((demand_row(_,_),decode(_,fresh(_,_)),stars_of(_,_),_>100,not(muted(_))))` |
| `head_column_type_conflict_is_refused` | unbuilt in both | `head_column_type_conflict(target/1,total,int,source/1,name,text)` |
| `higher_order_call_goal_rejected` | unbuilt in both | `dynamic_relation_name(call/3)` |
| `higher_order_call_over_atom_rejected` | unbuilt in both | `dynamic_relation_name(call/1)` |
| `host_input_column_shadows_runtime_witness` | unbuilt in both | `host_column_shadows_runtime(peek,input,witness_digest)` |
| `host_output_column_shadows_runtime_ordinal` | unbuilt in both | `host_column_shadows_runtime(look,output,ordinal)` |
| `int_out_of_range_is_named_unsupported` | unbuilt in both | `int_out_of_range(measure/1,value,9007199254740993)` |
| `int_rejects_fractional_ingress` | unbuilt in both | `type_arrival_shape_mismatch(measure/1,value,int,field_not_int(1.5))` |
| `json_array_groups_and_nests` | unbuilt in both | `aggregate_head(json_array(_))` |
| `json_array_keeps_bag_duplicates` | unbuilt in both | `aggregate_head(json_array(_))` |
| `json_capture_type_bool_is_refused` | unbuilt in both | `json_capture_type_unknown(bool)` |
| `json_capture_type_typo_is_refused` | unbuilt in both | `json_capture_type_unknown(itn)` |
| `json_each_fans_out` | unbuilt in both | `level_body_goal(repo_lang(_),json_each(_,_))` |
| `json_round_trip_decode_to_document` | unbuilt in both | `level_body_goal(repo_lang(_,_),json_each(_,_))` |
| `keep_on_non_log_rel_rejected` | unbuilt in both | `keep_on_non_log_rel(state/1)` |
| `key_range_reported_before_unknown_column_type` | unbuilt in both | `key_position_out_of_range(finding/2,3,2)` |
| `keyed_level_head_is_refused` | unbuilt in both | `keyed_level_head(current_value/2)` |
| `keyed_log_rejected` | unbuilt in both | `keyed_log_rel(latest/2,[1])` |
| `latest_in_level_rule_rejected` | unbuilt in both | `latest_in_level_rule(source_item/1)` |
| `list_interned_set_dictionary_content_deduplicates` | schedule serialization failure after both emitters succeed | `emitter returned false` |
| `list_interned_set_end_to_end` | schedule serialization failure after both emitters succeed | `emitter returned false` |
| `list_interned_set_relation_element_refused` | unbuilt in both | `list_interned_set_relation_element(fighter_summary)` |
| `list_of_relation_refs_still_refused` | unbuilt in both | `list_of_relation_refs(span)` |
| `log_on_level_headed_rel_rejected` | unbuilt in both | `log_on_level_headed_rel(derived_event/1)` |
| `log_without_retention_rejected` | unbuilt in both | `missing_retention(event/1)` |
| `match_enum_nonexhaustive_is_refused` | unbuilt in both | `match_nonexhaustive(body,redirect)` |
| `module_path_off_the_decl_tree_refuses` | unbuilt in both | `unresolvable_path([orchard,north,tree])` |
| `nested_head_without_a_parent_atom_refuses` | unbuilt in both | `nested_parent_unbound(orchard__tree)` |
| `non_array_value_at_list_column_is_refused` | unbuilt in both | `type_arrival_shape_mismatch(batch/2,payloads,json_list(json),field_not_array(42))` |
| `one_attempt_bounded_log_two_arms_refused` | unbuilt in both | `retention_head_conflict_risk(dispatch_first/2,count(1))` |
| `one_occurrence_two_rows_still_conflicts` | unbuilt in both | `edge_head_conflict_risk(latest/2,[ping/1])` |
| `option_companion_name_collision_is_named` | unbuilt in both | `option_companion_name_collision(pair_holder__before/1,pair_holder/2,before)` |
| `option_in_key_column_is_refused` | unbuilt in both | `option_in_key_column(session/2,token)` |
| `option_list_of_unknown_name_keeps_its_stop` | unbuilt in both | `column_type_unknown(fighter_summry)` |
| `option_of_interned_set_of_rel_is_refused` | unbuilt in both | `list_interned_set_relation_element(fighter_summary)` |
| `option_of_json_list_keeps_its_stop` | unbuilt in both | `option_element_type_unknown(json_list(int))` |
| `option_of_option_of_scalar_keeps_its_stop` | unbuilt in both | `option_element_type_unknown(option(int))` |
| `pipe_stage_costs_one_tick` | unbuilt in both | `edge_body_needs_json_destructure((demand_row(_,_),decode(_,fresh(_,_)),stars_of(_,_)))` |
| `pre_in_level_rule_rejected` | unbuilt in both | `pre_in_level_rule(source_item/1)` |
| `reference_target_emptied_by_option_split_is_named` | unbuilt in both | `reference_target_has_no_columns(squad/0)` |
| `regexp_operand_not_text` | unbuilt in both | `regexp_operand_not_text(source/1,value,int)` |
| `regexp_pattern_invalid` | unbuilt in both | `regexp_pattern_invalid("[","Syntax error: missing terminating ] for character class")` |
| `regexp_pattern_not_literal` | unbuilt in both | `regexp_pattern_not_literal` |
| `regexp_pattern_outside_subset` | unbuilt in both | `regexp_pattern_outside_subset("a(?=b)")` |
| `relation_pattern_target_arity_rejected` | unbuilt in both | `relation_pattern_not_a_relation_value(span/3,file,file,file(repo(acme)))` |
| `relation_pattern_text_literal_in_ref_column_rejected` | unbuilt in both | `relation_pattern_not_a_relation_value(span/3,file,file,'src/a.rs')` |
| `relation_pattern_wrong_target_rejected` | unbuilt in both | `relation_pattern_not_a_relation_value(span/3,file,file,fpath('a.rs'))` |
| `relation_ref_column_fed_by_text_variable_rejected` | unbuilt in both | `relation_column_type_conflict(span/3,file,file,raw3/3,file,text)` |
| `relation_value_in_edge_rule_rejected` | unbuilt in both | `relation_value_in_edge_rule(span/3,file,file,file(repo(acme),fpath('src/a.rs')))` |
| `relation_value_under_negation_rejected` | unbuilt in both | `relation_value_under_negation(span/3,file,file,file(repo(acme),fpath('missing.rs')))` |
| `repo_on_bind_watch_is_refused` | unbuilt in both | `bind_repo_column(watch)` |
| `reserved_namespace_declared_rel` | unbuilt in both | `reserved_rel_namespace('__txt_reach')` |
| `reserved_namespace_derived_head` | unbuilt in both | `reserved_rel_namespace('__str_stats')` |
| `retention_head_conflict_risk_rejected` | unbuilt in both | `retention_head_conflict_risk(journal/1,count(1))` |
| `same_tick_error_then_fresh_chains_arms` | unbuilt in both | `trigger_arg_not_var(error(_))` |
| `scan_is_a_named_unsupported` | unbuilt in both | `removed_word(scan)` |
| `scan_is_a_named_unsupported_at_five_arguments` | unbuilt in both | `removed_word(scan)` |
| `scope_done_three_spellings` | unbuilt in both | `trigger_arg_not_var(done)` |
| `seed_and_transition_are_disjoint` | unbuilt in both | `edge_body_with_negation((increment(_,_),not(pre(counter(_,_)))))` |
| `struct_arrival_field_type_rejected` | unbuilt in both | `type_arrival_shape_mismatch(finding/2,at,span,field_not_int(span,end,nine))` |
| `struct_arrival_functor_term_rejected` | unbuilt in both | `type_arrival_shape_mismatch(finding/2,at,span,not_an_object(span,span(3,9)))` |
| `struct_arrival_missing_key_rejected` | unbuilt in both | `type_arrival_shape_mismatch(finding/2,at,span,missing_key(span,end))` |
| `struct_arrival_unknown_key_rejected` | unbuilt in both | `type_arrival_shape_mismatch(finding/2,at,span,unknown_key(span,extra))` |
| `struct_column_type_unknown_rejected` | unbuilt in both | `column_type_unknown(spann)` |
| `struct_decode_field_unknown_rejected` | unbuilt in both | `decode_field_unknown(span,beginning)` |
| `struct_host_output_type_unknown_rejected` | unbuilt in both | `column_type_unknown(spann)` |
| `struct_type_cycle_rejected` | unbuilt in both | `type_cycle([node])` |
| `struct_type_mutual_cycle_rejected` | unbuilt in both | `type_cycle([left,right])` |
| `subscribe_is_a_named_unsupported` | unbuilt in both | `lifecycle_arm(subscribe)` |
| `text_one_and_numeric_one_are_not_equal` | unbuilt in both | `comparison_type_mismatch(_==_,text,int)` |
| `text_one_and_numeric_one_never_join` | unbuilt in both | `join_column_type_mismatch('b1."value"',int,'b0."value"',text)` |
| `text_rejects_number_ingress` | unbuilt in both | `type_arrival_shape_mismatch(label/1,value,text,field_not_text(4))` |
| `trigger_marker_is_what_stops_backlog_replay` | unbuilt in both | `edge_body_needs_json_destructure((demand_row(_,_),decode(_,fresh(_,_)),stars_of(_,_)))` |
| `typed_int_contradicts_text_witness` | unbuilt in both | `type_arrival_shape_mismatch(typed_conflict/1,value,int,field_not_int(text_value))` |
| `unmarked_chain_replays_to_late_subscriber` | unbuilt in both | `edge_body_needs_json_destructure((demand_row(_,_),decode(_,fresh(_,_)),stars_of(_,_)))` |
| `unmarked_first_stage_refires_on_late_watch` | unbuilt in both | `edge_body_needs_json_destructure((demand_row(_,_),decode(_,fresh(_,_)),stars_of(_,_)))` |
| `unsubscribe_is_a_named_unsupported` | unbuilt in both | `lifecycle_arm(unsubscribe)` |
| `wide_int_refused_at_undeclared_column` | unbuilt in both | `int_out_of_range(untyped/1,1,9007199254740993)` |
| `wide_int_refused_inside_json_document` | unbuilt in both | `int_out_of_range(payload/1,document,9007199254740993)` |
| `wrong_element_type_is_refused` | unbuilt in both | `type_arrival_shape_mismatch(batch/2,payloads,json_list(text),list_element_shape(2,field_not_text(42)))` |
| `zip_is_a_named_unsupported` | unbuilt in both | `zip` |

## Struct plane design

Measured result: `RUST-GRADE graded=392 byte-clean=248`, a transition from
230 to 248. The ratchet listed the same 18 fixtures assigned to the struct
plane before implementation. The remaining diff count was 32.

### Type signatures

The emitted program adds a list of struct type plans and a relation-to-reference
column map. The runtime entry point has this shape:

```rust
pub fn intern(
    seam: &SqliteSeam,
    types: &[StructTypePlan],
    ref_columns: &HashMap<String, Vec<Option<String>>>,
    arrivals: &[Arrival],
    relations: &[IncrementalRelationPlan],
    text_plan: Option<&TextInternPlan>,
) -> Vec<Arrival>
```

The body collects referenced values recursively, interns target rows in the
declared type order, records semantic-key-to-id results, and rewrites the
original parent arrivals.

### Instance timeline

Each tick creates collection maps and id maps. They live through one call to
`GenProgram::run_tick`. Database rows created for referenced targets live with
the SQLite seam and participate in the same tick frontier as authored arrivals.

### Storage and ordering

The collector reads each parent relation's reference-column plan, decodes the
wire object, validates keys, and visits referenced children before their
parent. For each type in the emitted topological list, the runtime writes target
arrivals, reads their dense ids, and records one id per semantic key. Parent
arrival rows are rewritten only after every required target id exists. A
target key may occur more than once in the input batch; the per-type map retains
one collected row for that semantic key.

## Departure frontier result

Departure staging moved the grade from 248 to 253. The five gained fixtures
are `departed_fires_next_tick_on_retraction`,
`finalize_over_log_fires_on_retention_prune`,
`keyed_replace_departs_the_old_row`,
`pairwise_pairs_adjacent_values_when_the_source_idles`, and
`pairwise_reads_state_at_the_departure_tick`.

The inherited six-row grouping included
`take_until_keyed_replace_negated_done`. Its source has no `finalize/1` arm at
`v6/prolog/conformance/fixtures/scopes.pl:273-296`, and its emitted TypeScript
dispatches to `run_ordered_tick` at
`v6/prolog/compile/out/take_until_keyed_replace_negated_done.ts:788-790`.
It remains among the 27 ordered-program diffs.

## Ordered program result

Ordered tick execution moved the grade from 253 to 280. All 27 diffs present
after departure staging became byte-clean. The emitted program now carries
ordered occurrence arms, trigger kinds, evolving `pre/1` targets, and the
separate delete and insert statements needed for recursive level closure.

### Type signatures

The emitted ordered arm has the runtime shape:

```rust
pub struct OrderedEdgeArm {
    pub trigger_rel: String,
    pub trigger_kind: OrderedTriggerKind,
    pub head_rel: String,
    pub head_kind: RelationKind,
    pub head_columns: Vec<String>,
    pub key_indices: Vec<usize>,
    pub project_sql: String,
    pub write_sql: String,
    pub evolves_pre: bool,
    pub intern_sql: Option<Vec<String>>,
}

pub fn run_tick(
    program: &GenProgram,
    seam: &SqliteSeam,
    arrivals: &[Arrival],
) -> TickDeltas
```

The ordered runtime reads carry, departure, authored, and level occurrences;
then applies each occurrence against its matching arm list in sequence.

### Instance timeline

The stored and decoded before snapshots live for one tick. The mid snapshot is
taken after authored arrivals, the `pre/1` snapshot, and the first level
closure. Each occurrence observes writes from earlier occurrences in the same
tick. The after snapshots are taken after the second level closure and
retention.

### Storage and ordering

Occurrence writes update relation tables immediately. Heads read through
`pre/1` also update the corresponding `__pre_*` table after each write. Exact
rows are deduplicated per head relation and keyed-set conflicts are checked per
head key. Ordered additions stage the next arrival frontier in occurrence
order. Net boundary deletions stage the departure frontier. Self-referential
level programs clear once and repeat their emitted insert statements until the
combined level row count stops changing.

## Verification

The struct implementation retained all 230 existing clean rows and added the
18 struct-plane rows. Every later cause is measured before and after its
commit. Final verification runs each command three times:

```text
bash v6/sprefa-engine-rs/grade.sh
just conformance
swipl -g go -t halt v6/prolog/ARCH.pl
cd v6/tsv2 && bash scripts/sweep.sh
cargo test --no-fail-fast
```

The report records exact grade counts, commit transitions, and the outputs of
all 15 gate runs. A gate duration above 10 seconds is recorded as a defect.

## Staffing

Primary Codex lane in
`.boop-worktrees/feature/emit-rust-climb-3`, based on `e70417d9`. No agent lanes
are assigned. The suite budget is three runs of each of the five requested
commands, with grade runs isolated from the whole repository gate.
