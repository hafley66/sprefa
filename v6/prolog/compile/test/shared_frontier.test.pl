% shared_frontier.test.pl : the frontier(shared) lowering option.
% Fail-first receipt: every unit here was red before lower.pl grew the
% shared-frontier block (unknown predicates / missing DDL text).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).

:- use_module(library(plunit)).

:- use_module('../../lower',
              [ frontier_mode/1, with_frontier_mode/2,
                shared_frontier_relation_id/3, lowered_program_data/2,
                lower_program/2 ]).
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
    memberchk(relation_data(1, person/2, _, [name, age], key([1]), materialized), Relations),
    memberchk(rule_data(0, 0, [1], pending), Rules).

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
