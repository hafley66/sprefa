% Actual oracle/compiler clock lab for automatic relation-edge membership.
%
% Run:
%   swipl -q -f v6/prolog/labs/rel_value_unification/10_reference_fixpoint_clock.pl

:- use_module('../../src/grader.pl').
:- use_module('../../1_expansion.pl', [expand_program/3]).
:- use_module('../../conformance/engine.pl',
              [run_program/5, rel_rows/3, rel_deltas/3]).
:- use_module('../../compile/compile.pl', [program_plan/2]).
:- use_module('../../compile/lower.pl', [lower_program/2]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).

go :- run(check).

:- initialization(go, main).

program_with_target_creation(
  prog([ type_decl(user, [col(id, int), col(name, text)]),
         col_type(user/2, id, int),
         col_type(user/2, name, text),
         keyed(user/2, [1]),
         col_type(post/1, author, user),
         kind(source/2, log),
         col_type(source/2, id, int),
         col_type(source/2, name, text),
         keep(source/2, all) ],
       [ (user(Id, Name) <+ source(Id, Name)),
         (post(user(Id, Name)) <- source(Id, Name)) ])).

program_without_target_creation(
  prog([ type_decl(user, [col(id, int), col(name, text)]),
         col_type(user/2, id, int),
         col_type(user/2, name, text),
         keyed(user/2, [1]),
         col_type(post/1, author, user),
         kind(source/2, log),
         col_type(source/2, id, int),
         col_type(source/2, name, text),
         keep(source/2, all) ],
       [ (post(user(Id, Name)) <- source(Id, Name)) ])).

plan(Program, Plan) :-
    program_plan(fixture(reference_fixpoint_clock,
                         Program, [], [], [])-[],
                 Plan).

check(level_parent_expands_to_target_membership,
      ( program_with_target_creation(Program),
        expand_program(Program, prog(_, Rules), _),
        member((post(user(Id, Name)) <-
                   (source(Id, Name), user(Id, Name))),
               Rules) )).

check(target_edge_precedes_parent_level_in_plan,
      ( program_with_target_creation(Program),
        plan(Program, plan(_, _, _, _, RuleOrder, EdgeRules)),
        member((user(_, _) <+ source(_, _)), EdgeRules),
        member((post(_) <- _), RuleOrder) )).

check(target_creation_and_parent_membership_settle_same_tick,
      ( program_with_target_creation(Program),
        run_program(Program, [],
                    [[+source(account, alice)]],
                    Final, Deltas),
        rel_rows(user/2, Final, [user(account, alice)]),
        rel_rows(post/1, Final, [post(user(account, alice))]),
        rel_deltas(post/1, Deltas,
                   [[+post(user(account, alice))], []]) )).

check(emitted_parent_projects_target_id_without_json,
      ( program_with_target_creation(Program),
        plan(Program, Plan),
        lower_program(Plan,
                      lowered(_, _, _, _, Levels, _, _, _)),
        member(levelstmt(post/1, _, Inserts, _, _, _), Levels),
        member(Sql, Inserts),
        sub_atom(Sql, _, _, _, '"user"'),
        sub_atom(Sql, _, _, _, '"__id"'),
        \+ sub_atom(Sql, _, _, _, 'json_object') )).

check(missing_target_produces_no_parent_membership,
      ( program_without_target_creation(Program),
        run_program(Program, [],
                    [[+source(account, alice)]],
                    Final, Deltas),
        rel_rows(post/1, Final, []),
        rel_deltas(post/1, Deltas, [[]]) )).

check(keyed_target_replacement_retracts_old_parent_same_tick,
      ( program_with_target_creation(Program),
        run_program(Program, [],
                    [[+source(account, alice)],
                     [+source(account, bob)]],
                    Final, Deltas),
        rel_rows(user/2, Final, [user(account, bob)]),
        rel_rows(post/1, Final, [post(user(account, bob))]),
        rel_deltas(post/1, Deltas,
                   [[+post(user(account, alice))],
                    [-post(user(account, alice)),
                     +post(user(account, bob))],
                    []]) )).
