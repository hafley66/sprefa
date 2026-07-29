% Actual compiler checks for the minimal relation-reference hypothesis.
%
% Run:
%   swipl -q -l v6/prolog/labs/rel_value_unification/5_reference_relation_holes.pl -g go -g halt

:- use_module('../../src/grader.pl').
:- use_module('../../compile/parse_dl.pl', [parse_dl_file/4, parse_dl/4]).
:- use_module('../../compile/compile.pl', [program_plan/2]).
:- use_module('../../compile/lower.pl', [lower_program/2]).

:- dynamic lab_directory/1.
:- discontiguous check/2.
:- prolog_load_context(directory, Directory), assertz(lab_directory(Directory)).

go :- run(check).

lab_program(Program, Bindings) :-
    lab_directory(Directory),
    directory_file_path(Directory, '4_reference_relation.dl6', Path),
    parse_dl_file(Path, Program, Bindings, []).

lab_lowered(Plan, Lowered) :-
    lab_program(Program, Bindings),
    program_plan(fixture(reference_relation_holes,
                         Program, [], [], [])-Bindings,
                 Plan),
    lower_program(Plan, Lowered).

check(reference_target_remains_public_rel,
      ( lab_lowered(plan(_, _, RelPlans, _, _, _), _),
        memberchk(relplan(span/2, set, [start, end], none, [int, int]),
                  RelPlans) )).

check(parent_column_is_typed_reference_edge,
      ( lab_lowered(plan(_, _, RelPlans, _, _, _), _),
        memberchk(relplan(finding/2, set, [path, at], none,
                          [text, ref(span)]),
                  RelPlans) )).

check(no_dictionary_or_stored_json_columns,
      ( lab_lowered(_, lowered(_, Ddl, _, _, _, _, _, _)),
        atomic_list_concat(Ddl, '\n', Sql),
        \+ sub_atom(Sql, _, _, _, '__dict_'),
        \+ sub_atom(Sql, _, _, _, '__semantic'),
        \+ sub_atom(Sql, _, _, _, '__rendered') )).

check(reference_target_has_one_ordinary_table,
      ( lab_lowered(_, lowered(_, Ddl, _, _, _, _, _, _)),
        include(has_create_span, Ddl, SpanCreates),
        SpanCreates = [_] )).

has_create_span(Sql) :-
    sub_atom(Sql, 0, _, _, 'CREATE TABLE "span" ').

check(direct_rhs_relation_query_compiles,
      ( lab_lowered(_, lowered(_, _, _, _, LevelStatements, _, _, _)),
        member(levelstmt(copied_span/2, _, Inserts, _, _, _),
               LevelStatements),
        member(Sql, Inserts),
        sub_atom(Sql, _, _, _, 'FROM "span"') )).

check(reference_dereference_is_an_indexed_relational_join,
      ( lab_lowered(_, lowered(_, _, _, _, LevelStatements, _, _, _)),
        member(levelstmt(finding_start/2, _, Inserts, _, _, _),
               LevelStatements),
        member(Sql, Inserts),
        sub_atom(Sql, _, _, _, '"__ref_span"'),
        sub_atom(Sql, _, _, _, '"__id"') )).

check(no_foreign_key_or_cascade_policy_invented,
      ( lab_lowered(_, lowered(_, Ddl, _, _, _, _, _, _)),
        atomic_list_concat(Ddl, '\n', Sql),
        \+ sub_atom(Sql, _, _, _, 'FOREIGN KEY'),
        \+ sub_atom(Sql, _, _, _, 'ON DELETE') )).

check(existing_key_is_not_yet_used_as_reference_identity,
      ( string_codes(
          "rel user(id: int, name: text) key(1).\nrel post(author: user).\n",
          Codes),
        parse_dl(Codes, Program, Bindings, []),
        program_plan(fixture(keyed_reference_hole,
                             Program, [], [], [])-Bindings,
                     Plan),
        lower_program(Plan, _),
        Plan = plan(_, prog(Decls, _), _, _, _, _),
        memberchk(keyed(user/2, [1]), Decls) )).

check(existing_keyed_cycle_is_still_refused,
      ( string_codes(
          "rel node(id: int, next: node) key(1).\nrel root(node: node).\n",
          Codes),
        parse_dl(Codes, Program, Bindings, []),
        catch(program_plan(fixture(keyed_cycle_hole,
                                   Program, [], [], [])-Bindings,
                           _),
              unsupported_construct(type_cycle([node])),
              Refused = yes),
        Refused == yes )).
