% lab.pl : the ONE self-loading entry point for the match/arrows/frontiers lab.
%
%   swipl -q -l v6/prolog/labs/match_frontier/lab.pl -g go -g halt
%
% Exits 0 printing ONLY `PASS <name>` lines. A FAIL line means the lab is not
% done. Contract: plans/2026-07-28-match-frontier-lab-header.md.
% Verdict:  plans/2026-07-28-match-frontier-lab-verdict.md.
%
% Nothing here edits v6/prolog/conformance/**; engine.pl, body.pl and
% level_eval.pl are consumed read-only exactly the way ticklog.pl consumes
% them.

:- use_module(library(lists)).
:- use_module(library(apply)).

:- use_module(desugar).
:- use_module(frontier).
:- use_module(lowering).
:- use_module(model).
:- use_module(scenarios).
:- use_module(syntax).
:- use_module(invariants).

:- dynamic failed/0.

check(Name, Goal) :-
    (   catch(Goal, Error, ( format("FAIL ~w threw ~q~n", [Name, Error]), fail ))
    ->  format("PASS ~w~n", [Name])
    ;   format("FAIL ~w~n", [Name]), assertz(failed)
    ).

go :-
    q1_checks, q2_checks, q3_checks, q4_checks, q5_checks, q6_checks,
    fixture_checks,
    ( failed -> halt(1) ; true ).

% ═══ Q1 : legality matrix ═══════════════════════════════════════════════════

q1_checks :-
    check(q1_every_cell_has_exactly_one_verdict, every_cell_once),
    check(q1_no_cell_is_blank, no_blank_cell),
    check(q1_legal_cells_desugar, legal_cells_desugar),
    check(q1_refuse_cells_throw_their_named_error, refuse_cells_throw),
    check(q1_ambiguous_cells_name_a_declared_slot, ambiguous_cells_slotted),
    check(q1_grid_is_nine_by_four, grid_size(9, 4, 36)).

every_cell_once :-
    forall(( matrix_row(Row), matrix_column(Column) ),
           ( findall(Verdict, legality(Row, Column, Verdict), Verdicts),
             Verdicts = [_] )).

no_blank_cell :-
    forall(( matrix_row(Row), matrix_column(Column) ),
           legality(Row, Column, _)).

legal_cells_desugar :-
    forall(( legality(Row, Column, legal(_)) ),
           ( desugar_cell(Row, Column, Kernel), nonvar(Kernel) )).

refuse_cells_throw :-
    forall(( legality(Row, Column, refuse(Expected)) ),
           ( catch(( desugar_cell(Row, Column, _), fail ), Thrown, true),
             Thrown == Expected )).

ambiguous_cells_slotted :-
    forall(( legality(Row, Column, ambiguous(Slot)) ),
           ( slot(Slot),
             catch(( desugar_cell(Row, Column, _), fail ), Thrown, true),
             Thrown == ambiguous(Slot) )).

grid_size(Rows, Columns, Cells) :-
    findall(Row, matrix_row(Row), RowList), length(RowList, Rows),
    findall(Column, matrix_column(Column), ColumnList), length(ColumnList, Columns),
    findall(Row2-Column2, legality(Row2, Column2, _), Grid), length(Grid, Cells).

% ═══ Q2 : frontier matrix ═══════════════════════════════════════════════════

q2_checks :-
    check(q2_every_cell_has_exactly_one_verdict, every_frontier_cell_once),
    check(q2_grid_is_twenty_four_cells, frontier_grid_size(24)),
    check(q2_tn_occurrences_are_refused_everywhere, tn_occurrences_refused),
    check(q2_every_ta_cell_is_incoherent_or_refused, ta_cells_never_coherent),
    check(q2_every_ti_occurrence_cell_is_drain_capped, ti_occurrence_drain_capped),
    check(q2_termination_values_are_from_the_named_set, termination_vocabulary).

every_frontier_cell_once :-
    forall(( frontier_axis(Axis), frontier_time(Time), head_kind(Kind) ),
           ( findall(Verdict, frontier(Axis, Time, Kind, Verdict, _, _), Verdicts),
             Verdicts = [_] )).

frontier_grid_size(Cells) :-
    findall(Axis-Time-Kind, frontier(Axis, Time, Kind, _, _, _), Grid),
    length(Grid, Cells).

tn_occurrences_refused :-
    forall(head_kind(Kind),
           ( frontier(occurrence, tn, Kind, Verdict, Termination, _),
             Verdict = refused(_), Termination == unbounded )).

ta_cells_never_coherent :-
    forall(( head_kind(Kind), frontier_axis(Axis) ),
           ( frontier(Axis, ta, Kind, Verdict, _, _),
             \+ Verdict = coherent(_) )).

ti_occurrence_drain_capped :-
    forall(head_kind(Kind),
           ( frontier(occurrence, ti, Kind, coherent(_), drain_capped, _) )).

termination_vocabulary :-
    forall(frontier(_, _, _, _, Termination, _),
           memberchk(Termination, [bounded, drain_capped, unbounded, stranded])).

% ═══ Q3 : rx directness ═════════════════════════════════════════════════════

q3_checks :-
    check(q3_every_legal_q1_cell_has_a_lowering, legal_cells_lowered),
    check(q3_every_coherent_q2_cell_has_a_lowering, coherent_cells_lowered),
    check(q3_every_grade_is_from_the_named_set, lowering_grades_valid),
    check(q3_counts_are_direct_24_vacuous_1_encoded_7_impossible_2,
          lowering_counts(24, 1, 7, 2)),
    check(q3_the_ta_scheduler_lowering_is_graded_vacuous_not_direct,
          lowering(ta_as_scheduler, _, direct_vacuous, _)),
    check(q3_the_pending_rel_lowering_is_direct,
          lowering(ta_as_pending_rel, _, direct, _)),
    check(q3_primitive_ta_lowering_needs_a_banned_subject_bridge,
          lowering(ta_as_primitive_queue, _, encoded, _)).

% A cell is covered either by its own key or by the (Row, any) wildcard key.
legal_cells_lowered :-
    forall(legality(Row, Column, legal(_)),
           ( lowering(cell(Row, Column), _, _, _) -> true
           ; lowering(cell(Row, any), _, _, _) )).

coherent_cells_lowered :-
    forall(frontier(Axis, Time, Kind, coherent(_), _, _),
           ( lowering(frontier(Axis, Time, Kind), _, _, _) -> true
           ; lowering(frontier(Axis, Time, any), _, _, _) )).

lowering_grades_valid :-
    forall(lowering(_, _, Grade, _), lowering_grade(Grade)).

lowering_counts(Direct, Vacuous, Encoded, Impossible) :-
    grade_count(direct, Direct),
    grade_count(direct_vacuous, Vacuous),
    grade_count(encoded, Encoded),
    grade_count(impossible, Impossible).

grade_count(Grade, Count) :-
    findall(Key, lowering(Key, _, Grade, _), Keys), length(Keys, Count).

% ═══ Q4 : contradiction hunt ════════════════════════════════════════════════

q4_checks :-
    forall(scenario(Name, Goal), check(Name, mf_scenarios:Goal)),
    check(q4_all_eight_mandatory_scenarios_present, mandatory_scenarios_present).

mandatory_scenarios_present :-
    forall(member(Prefix, [a1, a2, a3, a4, b1, b2, b3, c1, d1, d2,
                           e1, e2, f1, f2, f3, f4, g1, g2, h1, h2, x1, x2]),
           ( scenario(Name, _), atom_concat(Prefix, '_', Stem),
             atom_concat(Stem, _, Name), ! )).

% ═══ Q5 : syntax overload ═══════════════════════════════════════════════════

q5_checks :-
    forall(parse_receipt(Name, Goal), check(Name, mf_syntax:Goal)),
    check(q5_every_woe_has_at_least_two_alternatives, woes_have_two_alternatives),
    check(q5_every_woe_has_at_least_one_collision_row, woes_have_a_collision),
    check(q5_the_keep_existing_arrow_option_is_priced, keep_existing_arrow_priced),
    check(q5_the_ta_marker_woe_prices_five_options_no_at_first,
          ta_marker_options_priced),
    check(q5_severities_are_from_the_named_set, severity_vocabulary).

woes_have_two_alternatives :-
    forall(woe(Woe),
           ( findall(Spelling, alternative(Woe, Spelling, _, _), Spellings),
             length(Spellings, Count), Count >= 2 )).

woes_have_a_collision :-
    forall(woe(Woe), ( collision(Woe, _, _, _, _) -> true ; fail )).

% "keep <- and drop the mirrored arrows" must be priced as a real option, per
% the contract's judgment guidance.
keep_existing_arrow_priced :-
    alternative(woe_level_arm_arrow, Spelling, _, _),
    sub_atom(Spelling, _, _, _, 'keep <-'), !.

ta_marker_options_priced :-
    findall(Spelling, alternative(woe_ta_marker, Spelling, _, _), Spellings),
    length(Spellings, 5),
    Spellings = [First | _],
    sub_atom(First, _, _, _, 'NO MARKER AT ALL'),
    % the @ option exists but is last
    last(Spellings, Last), sub_atom(Last, _, _, _, '@async').

severity_vocabulary :-
    forall(collision(_, _, _, _, Severity), memberchk(Severity, [high, medium, low])).

% ═══ Q6 : invariants ════════════════════════════════════════════════════════

q6_checks :-
    check(q6_every_invariant_has_a_verdict_from_the_named_set, invariant_verdicts_valid),
    check(q6_every_broken_invariant_names_a_real_scenario, broken_names_a_scenario),
    check(q6_every_needs_rule_invariant_names_a_declared_slot, needs_rule_slotted),
    forall(invariant_check(Name, Goal),
           ( atom_concat('q6_', Name, CheckName), check(CheckName, mf_invariants:Goal) )),
    check(q6_all_eight_contract_invariants_are_covered, contract_invariants_covered).

invariant_verdicts_valid :-
    forall(invariant(_, Verdict, _),
           ( Verdict = preserved(_) ; Verdict = broken(_) ; Verdict = needs_rule(_) )).

broken_names_a_scenario :-
    forall(invariant(_, broken(Named), _),
           ( scenario(Named, _) -> true ; Named == carry_is_not_a_stored_rel )).

needs_rule_slotted :-
    forall(invariant(_, needs_rule(Slot), _), slot(Slot)).

contract_invariants_covered :-
    forall(member(Stem, [sugar_grounds_out, one_rel_one_rule_kind, stratification,
                         occurrence_multiplicity, r7_boundary_diff, retention_keep,
                         content_addressed_effect_identity, exactly_once_endurance]),
           ( invariant(Name, _, _), atom_concat(Stem, _, Name), ! )).

% ═══ prospective fixtures ═══════════════════════════════════════════════════

fixture_checks :-
    forall(prospective_fixture(Name, Goal), check(Name, mf_scenarios:Goal)),
    check(both_prospective_fixtures_present, two_prospective_fixtures).

two_prospective_fixtures :-
    findall(Name, prospective_fixture(Name, _), Names), length(Names, 2).
