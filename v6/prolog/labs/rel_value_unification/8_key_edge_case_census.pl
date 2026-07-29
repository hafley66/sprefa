% Actual compiler census for using existing key(...) as relation-edge identity.
%
% This characterizes the current world before key-driven reference lowering.
% Run:
%   swipl -q -f v6/prolog/labs/rel_value_unification/8_key_edge_case_census.pl

:- use_module('../../src/grader.pl').
:- use_module('../../compile/parse_dl.pl', [parse_dl/4]).
:- use_module('../../compile/compile.pl', [program_plan/2]).
:- use_module('../../compile/lower.pl', [lower_program/2]).

go :- run(check).

:- initialization(go, main).

parse_text(Text, Program, Bindings) :-
    string_codes(Text, Codes),
    parse_dl(Codes, Program, Bindings, []).

plan_text(Name, Text, Plan) :-
    parse_text(Text, Program, Bindings),
    program_plan(fixture(Name, Program, [], [], [])-Bindings, Plan).

lower_text(Name, Text, Plan, Lowered) :-
    plan_text(Name, Text, Plan),
    lower_program(Plan, Lowered).

user_program("rel user(id: int, name: text) key(1).\nrel post(author: user).\n").

composite_program("rel rev_file(revision: int, file: int, blob: int) key(1, 2).\nrel file_span(file: rev_file, start: int, end: int) key(1, 2, 3).\nrel finding(at: file_span).\n").

check(single_key_reaches_public_target_plan,
      ( user_program(Text),
        plan_text(single_key, Text,
                  plan(_, _, RelPlans, _, _, _)),
        memberchk(relplan(user/2, set, [id, name], key([1]),
                          [int, text]),
                  RelPlans) )).

check(single_key_becomes_target_unique_constraint,
      ( user_program(Text),
        lower_text(single_key_ddl, Text, _,
                   lowered(_, Ddl, _, _, _, _, _, _)),
        member(Sql, Ddl),
        sub_atom(Sql, 0, _, _, 'CREATE TABLE "user"'),
        sub_atom(Sql, _, _, _, 'UNIQUE ("id")') )).

check(composite_key_order_reaches_target_unique_constraint,
      ( composite_program(Text),
        lower_text(composite_key_ddl, Text, _,
                   lowered(_, Ddl, _, _, _, _, _, _)),
        member(Sql, Ddl),
        sub_atom(Sql, 0, _, _, 'CREATE TABLE "rev_file"'),
        sub_atom(Sql, _, _, _, 'UNIQUE ("revision", "file")') )).

check(reference_column_is_still_integer_endpoint,
      ( composite_program(Text),
        lower_text(composite_endpoint, Text, _,
                   lowered(_, Ddl, _, _, _, _, _, _)),
        member(Sql, Ddl),
        sub_atom(Sql, 0, _, _, 'CREATE TABLE "finding"'),
        sub_atom(Sql, _, _, _, '"at" INTEGER NOT NULL') )).

check(keyed_arrival_replaces_by_key,
      ( user_program(Text),
        lower_text(keyed_arrival, Text, _,
                   lowered(_, _, Arrivals, _, _, _, _, _)),
        member(arrivalstmt(user/2, set, AddSql, DelSql, _, _), Arrivals),
        sub_atom(AddSql, 0, _, _, 'INSERT OR REPLACE'),
        sub_atom(DelSql, _, _, _, '"id" = ?'),
        sub_atom(DelSql, _, _, _, '"name" = ?') )).

check(stale_retraction_is_exact_row_not_key_retraction,
      ( user_program(Text),
        lower_text(stale_retract, Text, _,
                   lowered(_, _, Arrivals, _, _, _, _, _)),
        member(arrivalstmt(user/2, set, _, DelSql, _, _), Arrivals),
        sub_atom(DelSql, _, _, _, '"id" = ? AND "name" = ?') )).

check(keyed_self_cycle_is_currently_refused,
      ( Text = "rel node(id: int, next: node) key(1).\nrel root(node: node).\n",
        parse_text(Text, Program, Bindings),
        catch(program_plan(fixture(keyed_self_cycle,
                                   Program, [], [], [])-Bindings,
                           _),
              unsupported_construct(type_cycle([node])),
              Refused = yes),
        Refused == yes )).

check(keyed_mutual_cycle_is_currently_refused,
      ( Text = "rel left(id: int, right: right) key(1).\nrel right(id: int, left: left) key(1).\nrel root(left: left).\n",
        parse_text(Text, Program, Bindings),
        catch(program_plan(fixture(keyed_mutual_cycle,
                                   Program, [], [], [])-Bindings,
                           _),
              unsupported_construct(type_cycle([left, right])),
              Refused = yes),
        Refused == yes )).

check(zero_key_position_is_not_rejected_at_plan_time,
      ( Text = "rel user(id: int, name: text) key(0).\n",
        plan_text(zero_key_position, Text,
                  plan(_, _, RelPlans, _, _, _)),
        memberchk(relplan(user/2, set, [id, name], key([0]),
                          [int, text]),
                  RelPlans) )).

check(out_of_range_key_position_is_not_rejected_at_plan_time,
      ( Text = "rel user(id: int, name: text) key(3).\n",
        plan_text(out_of_range_key_position, Text,
                  plan(_, _, RelPlans, _, _, _)),
        memberchk(relplan(user/2, set, [id, name], key([3]),
                          [int, text]),
                  RelPlans) )).

check(duplicate_key_positions_are_not_rejected_at_plan_time,
      ( Text = "rel user(id: int, name: text) key(1, 1).\n",
        plan_text(duplicate_key_positions, Text,
                  plan(_, _, RelPlans, _, _, _)),
        memberchk(relplan(user/2, set, [id, name], key([1, 1]),
                          [int, text]),
                  RelPlans) )).

check(construction_still_uses_full_relation_arity_and_json,
      ( Text = "rel user(id: int, name: text) key(1).\nrel post(author: user).\npost(user(Id, Name)) <- user(Id, Name).\n",
        lower_text(full_constructor, Text, _,
                   lowered(_, _, _, _, Levels, _, _, _)),
        member(levelstmt(post/1, _, Inserts, _, _, _), Levels),
        member(Sql, Inserts),
        sub_atom(Sql, _, _, _, 'json_object'),
        sub_atom(Sql, _, _, _, '''user''') )).
