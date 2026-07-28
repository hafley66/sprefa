% Standalone hosts and extraction lab.
%
% Run from v6/prolog:
%   swipl -q -l labs/hosts_extraction/lab.pl -g go -g halt

:- op(1150, xfx, <-).

:- use_module('../../conformance/engine.pl').
:- use_module('0_terms.pl',
              [ ghcacher_program/1,
                fetch_template/1,
                fetch_explicit_decl/1,
                fetch_inferred_decl/1,
                native_ts_query/1,
                ast_grep_example/1
              ]).
:- use_module('1_hosts.pl').
:- use_module('2_json.pl').
:- use_module('3_extraction.pl').
:- use_module('4_query_terms.pl').

:- dynamic failed/0.
:- discontiguous check/2.

fixture(Name, Prog, Initial, Schedule, Expectations) :-
    hosts_extraction_terms:fixture(Name, Prog, Initial, Schedule, Expectations).

run_check(Name, Goal) :-
    ( catch(Goal, Error,
            ( format(user_error, "FAIL ~w threw ~q~n", [Name, Error]), fail ))
    -> format("PASS ~w~n", [Name])
    ;  format(user_error, "FAIL ~w~n", [Name]),
       assertz(failed)
    ).

go :-
    retractall(failed),
    forall(check(Name, Goal), run_check(Name, Goal)),
    ( failed -> halt(1) ; true ).

% Q1: explicit and inferred host declarations, probe identity, and refusals.
check(q1_explicit_input_output_split_compiles,
      ( fetch_explicit_decl(Decl),
        compile_host_decl(Decl,
                          host_plan(fetch,
                                    [col(ep, text), col(prev, text)],
                                    [ col(status, int), col(tag, text),
                                      col(body, text) ],
                                    template("template with {ep} and $prev"))) )).

check(q1_inferred_split_reaches_the_same_plan,
      ( fetch_explicit_decl(Explicit),
        fetch_inferred_decl(Inferred),
        compile_host_decl(Explicit, Plan),
        compile_host_decl(Inferred, Plan) )).

check(q1_probe_mints_content_identity_and_salted_witness,
      ( fetch_explicit_decl(Decl),
        compile_host_decl(Decl, Plan),
        demand_row(Plan,
                   probe(fetch, ["repo", "etag"], [Status, Tag, Body],
                         [salt(bucket, 9)]),
                   request(fetch,
                           identity_digest(host(fetch, Inputs)),
                           witness_digest(host(fetch, Inputs, [salt(bucket, 9)]))),
                   response_shape(
                     [ field(status, int, Status), field(tag, text, Tag),
                       field(body, text, Body) ])) )).

check(q1_same_content_and_salt_share_one_request,
      ( fetch_explicit_decl(Decl),
        compile_host_decl(Decl, Plan),
        Probe = probe(fetch, ["repo", "etag"], [_, _, _], [salt(bucket, 9)]),
        demand_row(Plan, Probe, RequestA, _),
        demand_row(Plan, Probe, RequestB, _),
        RequestA =@= RequestB )).

check(q1_new_salt_keeps_identity_and_changes_witness,
      ( fetch_explicit_decl(Decl),
        compile_host_decl(Decl, Plan),
        demand_row(Plan, probe(fetch, ["repo", "etag"], [_, _, _],
                               [salt(bucket, 9)]),
                   request(fetch, Identity, WitnessA), _),
        demand_row(Plan, probe(fetch, ["repo", "etag"], [_, _, _],
                               [salt(bucket, 10)]),
                   request(fetch, Identity, WitnessB), _),
        WitnessA \== WitnessB )).

check(q1_refuses_unreferenced_input,
      refusal(
        compile_host_decl(
          sh_decl(fetch, [col(ep, text), col(prev, text)],
                  [col(status, int)], template("template with {ep}")),
          _),
        template_mismatch(unreferenced_input(prev)))).

check(q1_refuses_output_used_as_input,
      refusal(
        compile_host_decl(
          sh_decl(fetch, [col(ep, text)],
                  [col(status, int)], template("{ep} and $status")),
          _),
        template_mismatch(output_used_as_input(status)))).

check(q1_refuses_unknown_brace_column,
      refusal(
        compile_host_decl(
          sh_decl(fetch, [col(ep, text)],
                  [col(status, int)], template("{ep} and {missing}")),
          _),
        template_mismatch(unknown_column(missing)))).

check(q1_refuses_input_output_overlap,
      refusal(
        compile_host_decl(
          sh_decl(fetch, [col(ep, text)], [col(ep, text)],
                  template("{ep}")),
          _),
        column_mismatch(input_output_overlap(ep)))).

check(q1_refuses_probe_arity_mismatch,
      ( fetch_explicit_decl(Decl),
        compile_host_decl(Decl, Plan),
        refusal(demand_row(Plan, probe(fetch, ["repo"], [_, _, _], []), _, _),
                probe_mismatch(_)) )).

check(q1_dollar_reference_requires_identifier_boundary,
      refusal(
        compile_host_decl(
          sh_decl(fetch, [col(prev, text)], [col(status, int)],
                  template("$previous")),
          _),
        template_mismatch(unreferenced_input(prev)))).

% Q2: explicit bind declarations remove rel-name magic and extend EDB origin.
check(q2_zero_decl_magic_bind_hazard_reproduced,
      ( Program = program([rel_decl(interval,
                                    [col(period, int), col(bucket, int)])],
                          [], []),
        Registry = [bind_def(interval,
                             [col(period, int), col(bucket, int)])],
        active_magic_binds(Registry, Program, [interval]) )).

check(q2_explicit_bind_decl_activates_by_name_and_shape,
      ( Program = program(
          [ rel_decl(interval, [col(period, int), col(bucket, int)]),
            bind_decl(interval, [col(period, int), col(bucket, int)])
          ], [], []),
        Registry = [bind_def(interval,
                             [col(period, int), col(bucket, int)])],
        active_declared_binds(Registry, Program, [interval]) )).

check(q2_rel_without_bind_decl_does_not_activate,
      ( Program = program([rel_decl(interval,
                                    [col(period, int), col(bucket, int)])],
                          [], []),
        Registry = [bind_def(interval,
                             [col(period, int), col(bucket, int)])],
        active_declared_binds(Registry, Program, []) )).

check(q2_bind_decl_makes_edb_by_declaration,
      ( Program = program(
          [ rel_decl(interval, [col(period, int), col(bucket, int)]),
            bind_decl(interval, [col(period, int), col(bucket, int)])
          ], [], []),
        rel_origin(interval, Program, edb(bind_declaration)) )).

check(q2_never_headed_plain_rel_remains_edb_by_absence,
      ( Program = program([rel_decl(input, [col(value, text)])], [], []),
        rel_origin(input, Program, edb(never_headed)) )).

check(q2_bind_and_rule_head_is_refused,
      ( Program = program(
          [ rel_decl(interval, [col(period, int), col(bucket, int)]),
            bind_decl(interval, [col(period, int), col(bucket, int)])
          ],
          [rule(interval(Period, Bucket), [seed(Period, Bucket)])], []),
        rel_origin(interval, Program, refused(bind_and_rule_head(interval))) )).

% Q3: the surface query retains its complete relation atom.
check(q3_query_term_compiles,
      compile_query(query(change_log(Ep, Kind, Value)),
                    query_plan(change_log/3, columns([Ep, Kind, Value]),
                               snapshot(current)))).

check(q3_ghcacher_program_ends_in_query_term,
      ( ghcacher_program(program(_, _, Queries)),
        Queries = [query(change_log(_, _, _))] )).

% Q4: actual landed JSON semantics plus explicit text-decoder residue.
check(q4_jsonp_field_pull_and_array_explode_run,
      ( fixture(ghcacher_json_normalization, _, _, _, Expectations),
        engine:fixture_expectations_hold(ghcacher_json_normalization,
                                         Expectations) )).

check(q4_sibling_and_nested_fields_stay_correlated,
      ( fixture(ghcacher_json_normalization, Prog, Initial, Schedule, _),
        engine:run_program(Prog, Initial, Schedule, Final, _),
        engine:rel_rows(pull_request/5, Final,
                        [ pull_request(pulls, 7, "seven", "open", "octo"),
                          pull_request(pulls, 8, "eight", "closed", "hub")
                        ]) )).

check(q4_both_rx_lowerings_are_named,
      ( json_rx_lowering(field_pull, _),
        json_rx_lowering(array_explode, _) )).

check(q4_text_to_json_value_residue_is_named,
      json_residue(slot_json_text_to_value)).

% Q5: same examples, both shapes, delta and sharing receipts.
check(q5_callgraph_delta_size_equal_both_ways,
      ( changed_file_delta(callgraph, host, Host),
        changed_file_delta(callgraph, term_extract, Term),
        Host == Term,
        length(Host, 2) )).

check(q5_span_line_delta_size_equal_both_ways,
      ( changed_file_delta(span_line, host, Host),
        changed_file_delta(span_line, term_extract, Term),
        Host == Term,
        length(Host, 2) )).

check(q5_host_shares_content_salt_across_rules,
      extraction_invocations(host, d1, q_calls, 1)).

check(q5_term_op_runs_per_rule_occurrence,
      extraction_invocations(term_extract, d1, q_calls, 2)).

check(q5_edge_feed_shapes_are_explicit,
      ( fork_grade(host, edge_feed, direct_from_edb_delta),
        fork_grade(term_extract, edge_feed, via_materialized_level_rel) )).

check(q5_row_mint_points_are_explicit,
      ( fork_grade(host, row_mint, world_boundary),
        fork_grade(term_extract, row_mint, rule_evaluation) )).

check(q5_both_rx_lowerings_are_honest,
      ( extraction_rx_lowering(host, HostRx),
        extraction_rx_lowering(term_extract, TermRx),
        sub_string(HostRx, _, _, _, "runHostExtract"),
        sub_string(TermRx, _, _, _, "extract(content, query)") )).

% Q6: native query term fidelity and ast-grep's separate term family.
check(q6_native_query_compiles_exactly,
      ( native_ts_query(Query),
        compile_ts_query(Query, Text),
        Text ==
"((call_expression function: (identifier) @callee arguments: (arguments [(_) @arg \",\"]+)) (#eq? @callee \"fetch\") (#match? @arg \"^[a-z]+$\"))\n(comment)?\n_*" )).

check(q6_every_required_feature_has_a_term_and_slot,
      ( native_ts_query(Query),
        forall(feature_slot(Feature, mapped(_)),
               query_has_feature(Query, Feature)) )).

check(q6_unknown_ts_form_refuses_with_named_slot,
      refusal(compile_ts_query(ts_query([directive(set, "x")]), _),
              unmapped_feature(slot_ts_pattern_form,
                               directive(set, "x")))).

check(q6_ast_grep_uses_its_own_term_shape,
      ( ast_grep_example(Pattern),
        compile_sg_pattern(Pattern,
                           sg_plan(rust, "$RECEIVER.unwrap()", [receiver])),
        refusal(compile_ts_query(Pattern, _),
                unmapped_feature(slot_sg_metavariable_semantics, Pattern)) )).

check(q6_ts_and_sg_rx_lowerings_are_named,
      ( query_rx_lowering(_), sg_rx_lowering(_) )).

% Q7: the executable slot inventory distilled in the verdict.
check(q7_a12_resolved_push_bind_differs_from_demand_host,
      ambiguity(a12, resolved(push_bind_is_distinct_from_demand_host))).
check(q7_a1_resolved_glob_is_host_demand_column,
      ambiguity(a1, resolved(glob_is_host_demand_column))).
check(q7_a4_open_for_non_ts_embedded_text,
      ambiguity(a4, open(slot_general_embedded_text_escape))).
check(q7_a14_open_on_trailing_comment_bind_receipt,
      ambiguity(a14, open(slot_comment_span_trailing_bind))).

ambiguity(a12, resolved(push_bind_is_distinct_from_demand_host)).
ambiguity(a1, resolved(glob_is_host_demand_column)).
ambiguity(a4, open(slot_general_embedded_text_escape)).
ambiguity(a14, open(slot_comment_span_trailing_bind)).

% Whole-program receipt and fixture/5 candidate inventory.
check(full_ghcacher_program_compiles_in_lab_model,
      ( ghcacher_program(Program),
        compile_program(Program,
                        compiled([host_plan(fetch, _, _, _)],
                                 [interval],
                                 [query_plan(change_log/3, _, _)])) )).

check(five_fixture_candidates_are_distilled,
      ( findall(Name, fixture(Name, _, _, _, _), Names),
        sort(Names, Sorted),
        Sorted == [ extraction_fork_callgraph, extraction_fork_span_line,
                    ghcacher_host_program_term, ghcacher_json_normalization,
                    native_ts_query_term ] )).
