% shared_frontier.test.pl : the frontier(shared) lowering option.
% Fail-first receipt: every unit here was red before lower.pl grew the
% shared-frontier block (unknown predicates / missing DDL text).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).

:- use_module(library(plunit)).

:- use_module('../../next/2_lower/lower',
              [ frontier_mode/1, with_frontier_mode/2,
                shared_frontier_relation_id/3, lowered_program_data/2,
                write_verb/1, lower_program/2 ]).
:- use_module('../../compile', [ program_plan/2 ]).

:- begin_tests(shared_frontier).

test(mode_defaults_to_per_rel) :-
    frontier_mode(per_rel).

test(mode_scopes_to_the_wrapper) :-
    with_frontier_mode(shared, frontier_mode(shared)),
    frontier_mode(per_rel).

test(relation_ids_follow_relplan_order) :-
    tiny_plan(Plan),
    Plan = plan(_, _, _, RelPlans, _, _, _, _, _),
    once(shared_frontier_relation_id(RelPlans, adult/1, 0)),
    once(shared_frontier_relation_id(RelPlans, person/2, 1)).

test(shared_ddl_replaces_per_rel_frontier_tables) :-
    tiny_plan(Plan),
    once(with_frontier_mode(shared,
                            lower_program(Plan, lowered(_, Ddl, _, _, _, _, _, _)))),
    memberchk('CREATE TEMP TABLE "__frontier" ("relation_id" INTEGER NOT NULL, "_phase" INTEGER NOT NULL, "_sequence" INTEGER NOT NULL, "row_id" INTEGER NOT NULL)', Ddl),
    \+ ( member(Statement, Ddl),
         sub_atom(Statement, _, _, _, 'CREATE TEMP TABLE "__frontier_') ),
    forall(( member(Statement, Ddl),
             sub_atom(Statement, 0, _, _, 'CREATE TEMP VIEW "__frontier_') ),
           sub_atom(Statement, _, _, _, '"relation_id" = ')).

test(per_rel_ddl_untouched_without_the_option) :-
    tiny_plan(Plan),
    once(lower_program(Plan, lowered(_, Ddl, _, _, _, _, _, _))),
    \+ memberchk('CREATE TEMP TABLE "__frontier" ("relation_id" INTEGER NOT NULL, "_phase" INTEGER NOT NULL, "_sequence" INTEGER NOT NULL, "row_id" INTEGER NOT NULL)', Ddl).

test(program_data_projects_relations_and_rules) :-
    tiny_plan(Plan),
    once(lowered_program_data(Plan, program_data(Relations, Rules, [], [], []))),
    memberchk(relation_data(1, person/2, _, [name, age], key([1]), materialized,
                            _),
              Relations),
    memberchk(rule_data(0, 0, [1], _), Rules).

test(every_relation_row_carries_the_five_relation_verbs) :-
    tiny_plan(Plan),
    once(lowered_program_data(Plan, program_data(Relations, _, _, _, _))),
    forall(member(relation_data(_, _, _, _, _, _, Verbs), Relations),
           ( memberchk(verb(arrive, _), Verbs),
             memberchk(verb(stage, _), Verbs),
             memberchk(verb(read_staged, _), Verbs),
             memberchk(verb(publish, _), Verbs),
             memberchk(verb(clear, _), Verbs) )).

test(rule_rows_carry_the_recount_verb_with_its_join_sql) :-
    tiny_plan(Plan),
    once(lowered_program_data(Plan, program_data(_, Rules, _, _, _))),
    once(memberchk(rule_data(0, 0, [1], [verb(recount, recount(SeedSql, none))]),
                   Rules)),
    once(sub_atom(SeedSql, _, _, _, 'INSERT INTO "__support_next_')),
    once(sub_atom(SeedSql, _, _, _, '"age" >= 18')).

test(the_six_verbs_are_the_named_set) :-
    findall(Verb, write_verb(Verb), Verbs),
    Verbs == [arrive, stage, read_staged, recount, publish, clear].

% The whole point of the TEMP views: a compiled read keeps its text when the
% rows move to the shared table.
test(read_staged_text_is_identical_in_both_modes) :-
    tiny_plan(Plan),
    once(lowered_program_data(Plan, program_data(PerRelRelations, _, _, _, _))),
    once(with_frontier_mode(shared,
                            lowered_program_data(
                                Plan,
                                program_data(SharedRelations, _, _, _, _)))),
    forall(( member(relation_data(Id, _, _, _, _, _, PerRelVerbs),
                    PerRelRelations),
             memberchk(verb(read_staged, sql(PerRelSql)), PerRelVerbs) ),
           ( memberchk(relation_data(Id, _, _, _, _, _, SharedVerbs),
                       SharedRelations),
             memberchk(verb(read_staged, sql(SharedSql)), SharedVerbs),
             PerRelSql == SharedSql )).

test(stage_verb_names_the_shared_relation_id_under_shared) :-
    tiny_plan(Plan),
    once(with_frontier_mode(shared,
                            lowered_program_data(
                                Plan,
                                program_data(Relations, _, _, _, _)))),
    memberchk(relation_data(1, person/2, _, _, _, _, Verbs), Relations),
    memberchk(verb(stage, shared_frontier(1)), Verbs).

test(shared_ddl_carries_the_support_ledger) :-
    tiny_plan(Plan),
    once(with_frontier_mode(shared,
                            lower_program(Plan, lowered(_, Ddl, _, _, _, _, _, _)))),
    memberchk('CREATE TEMP TABLE "__support_count" ("relation_id" INTEGER NOT NULL, "row_id" INTEGER NOT NULL, "rule_id" INTEGER NOT NULL, "count" INTEGER NOT NULL, PRIMARY KEY ("relation_id", "row_id", "rule_id")) WITHOUT ROWID', Ddl).

test(recount_verb_publishes_per_rule_support_under_shared) :-
    tiny_plan(Plan),
    once(with_frontier_mode(shared,
                            lowered_program_data(
                                Plan,
                                program_data(_, Rules, _, _, _)))),
    once(memberchk(rule_data(0, 0, [1],
                             [verb(recount,
                                   recount(_, support_count(ClearSql,
                                                            [WriteSql])))]),
                   Rules)),
    ClearSql == 'DELETE FROM "__support_count" WHERE "relation_id" = 0',
    once(sub_atom(WriteSql, _, _, _, 'INSERT INTO "__support_count" ("relation_id", "row_id", "rule_id", "count")')),
    once(sub_atom(WriteSql, _, _, _, 'h."__id"')).

test(per_rel_rules_carry_no_support_ledger) :-
    tiny_plan(Plan),
    once(lowered_program_data(Plan, program_data(_, Rules, _, _, _))),
    forall(member(rule_data(_, _, _, [verb(recount, recount(_, Support))]),
                  Rules),
           Support == none).

test(edge_rules_refuse_loudly_under_shared,
     [ throws(unsupported_construct(frontier_shared_todo(edge_rules))) ]) :-
    program_plan(
        fixture(shared_frontier_edge,
                prog([ col_type(ping/1, value, int),
                       col_type(pong/1, value, int),
                       keyed(pong/1, [1]) ],
                     [ (pong(Value) <+ ping(Value)) ]),
                [], [], []) - [],
        EdgePlan),
    with_frontier_mode(shared, lower_program(EdgePlan, _)).

tiny_plan(Plan) :-
    once(program_plan(
        fixture(shared_frontier_tiny,
                prog([ col_type(person/2, name, text),
                       col_type(person/2, age, int),
                       keyed(person/2, [1]) ],
                     [ (adult(Name) <- person(Name, Age), Age >= 18) ]),
                [], [], []) - [],
        Plan)).

:- end_tests(shared_frontier).
