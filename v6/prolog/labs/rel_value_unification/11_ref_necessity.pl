% Actual compiler receipts for whether opaque row identity needs ref syntax.
%
% Run:
%   swipl -q -f v6/prolog/labs/rel_value_unification/11_ref_necessity.pl

:- use_module('../../src/grader.pl').
:- use_module('../../compile/parse_dl.pl', [parse_dl/4]).
:- use_module('../../compile/compile.pl', [program_plan/2]).
:- use_module('../../compile/lower.pl', [lower_program/2]).
:- use_module('../../compile/registry', [surface_for_term/6]).

go :- run(check).

:- initialization(go, main).

parse_text(Text, Program, Bindings) :-
    string_codes(Text, Codes),
    parse_dl(Codes, Program, Bindings, []).

lower_text(Name, Text, Plan, Lowered) :-
    parse_text(Text, Program, Bindings),
    program_plan(fixture(Name, Program, [], [], [])-Bindings, Plan),
    lower_program(Plan, Lowered).

check(target_scan_captures_dense_identity_without_ref,
      ( Text = "rel user(id: int, name: text) key(1).\nrel selected(choice: user).\nselected(user(Id, Name)) <- user(Id, Name).\n",
        lower_text(capture_identity, Text, _,
                   lowered(_, _, _, _, Levels, _, _, _)),
        member(levelstmt(selected/1, _, Inserts, _, _, _), Levels),
        member(Sql, Inserts),
        sub_atom(Sql, _, _, _, 'SELECT b0."__id" FROM "user" b0'),
        \+ sub_atom(Sql, _, _, _, 'json_object') )).

check(incremental_target_frontier_rejoins_dense_identity_without_json,
      ( Text = "rel user(id: int, name: text) key(1).\nrel selected(choice: user).\nselected(user(Id, Name)) <- user(Id, Name).\n",
        lower_text(incremental_identity, Text, _,
                   lowered(_, _, _, _, Levels, _, _, _)),
        member(levelstmt(selected/1, _, _, DeltaSql, _, _), Levels),
        sub_atom(DeltaSql, _, _, _, 'SELECT DISTINCT r0."__id"'),
        sub_atom(DeltaSql, _, _, _, 'FROM "__frontier_user" d0, "user" r0'),
        sub_atom(DeltaSql, _, _, _, 'r0."id" = d0."id"'),
        sub_atom(DeltaSql, _, _, _, 'r0."name" = d0."name"'),
        \+ sub_atom(DeltaSql, _, _, _, 'json_object') )).

check(typed_variable_forwards_opaque_identity_without_target_rejoin,
      ( Text = "rel user(id: int, name: text) key(1).\nrel selected(choice: user).\nrel pinned(choice: user).\nselected(user(Id, Name)) <- user(Id, Name).\npinned(Choice) <- selected(Choice).\n",
        lower_text(forward_identity, Text, _,
                   lowered(_, _, _, _, Levels, _, _, _)),
        member(levelstmt(pinned/1, _, Inserts, _, _, _), Levels),
        member(Sql, Inserts),
        sub_atom(Sql, _, _, _, 'SELECT b0."choice" FROM "selected" b0'),
        \+ sub_atom(Sql, _, _, _, 'JOIN "user"') )).

check(existing_decode_destructures_opaque_identity,
      ( Text = "rel user(id: int, name: text) key(1).\nrel selected(choice: user).\nrel selected_name(name: text).\nselected_name(Name) <- selected(Choice), decode(Choice, {name: Name}).\n",
        lower_text(decode_identity, Text, _,
                   lowered(_, _, _, _, Levels, _, _, _)),
        member(levelstmt(selected_name/1, _, Inserts, _, _, _), Levels),
        member(Sql, Inserts),
        sub_atom(Sql, _, _, _, 'FROM "selected" b0, "__ref_user" b1'),
        sub_atom(Sql, _, _, _, 'b1."__id" = b0."choice"') )).

check(separate_edge_relation_represents_graph_cycles,
      ( Text = "rel node(id: int, name: text) key(1).\nrel next(from: node, to: node) key(1, 2).\n",
        lower_text(graph_cycle_schema, Text, _,
                   lowered(_, Ddl, _, _, _, _, _, _)),
        member(NextDdl, Ddl),
        sub_atom(NextDdl, 0, _, _, 'CREATE TABLE "next"'),
        sub_atom(NextDdl, _, _, _, '"from" INTEGER NOT NULL'),
        sub_atom(NextDdl, _, _, _, '"to" INTEGER NOT NULL'),
        sub_atom(NextDdl, _, _, _, 'PRIMARY KEY ("from", "to")') )).

check(recursive_inline_reference_remains_named_refusal,
      ( Text = "rel node(id: int, next: node) key(1).\n",
        parse_text(Text, Program, Bindings),
        catch(program_plan(fixture(recursive_inline,
                                   Program, [], [], [])-Bindings,
                           _),
              unsupported_construct(type_cycle([node])),
              Refused = yes),
        Refused == yes )).

check(ref_has_no_registered_surface_semantics,
      \+ surface_for_term(ref/1, _, _, _, _, _)).
