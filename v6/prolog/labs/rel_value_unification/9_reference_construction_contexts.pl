% Actual compiler census of relation-edge construction contexts.
%
% Run:
%   swipl -q -f v6/prolog/labs/rel_value_unification/9_reference_construction_contexts.pl

:- use_module('../../src/grader.pl').
:- use_module('../../compile/parse_dl.pl', [parse_dl/4]).
:- use_module('../../compile/compile.pl', [program_plan/2]).
:- use_module('../../compile/lower.pl',
              [ lower_program/2, struct_type_plans/2,
                boot_statements/5 ]).

go :- run(check).

:- initialization(go, main).

parse_text(Text, Program, Bindings) :-
    string_codes(Text, Codes),
    parse_dl(Codes, Program, Bindings, []).

plan_text(Name, Text, Initial, Plan) :-
    parse_text(Text, Program, Bindings),
    program_plan(fixture(Name, Program, Initial, [], [])-Bindings, Plan).

lower_text(Name, Text, Initial, Plan, Lowered) :-
    plan_text(Name, Text, Initial, Plan),
    lower_program(Plan, Lowered).

program_text("rel user(id: int, name: text) key(1).\nrel post(author: user).\n").

check(existing_target_query_has_target_id_available_in_sql,
      ( Text = "rel user(id: int, name: text) key(1).\nrel post(author: user).\nrel source(id: int, name: text).\npost(user(Id, Name)) <- source(Id, Name), user(Id, Name).\n",
        lower_text(existing_target, Text, [], _,
                   lowered(_, _, _, _, Levels, _, _, _)),
        member(levelstmt(post/1, _, Inserts, _, _, _), Levels),
        member(Sql, Inserts),
        sub_atom(Sql, _, _, _, 'FROM "source"'),
        sub_atom(Sql, _, _, _, '"user"') )).

check(existing_target_constructor_projects_available_id_without_json,
      ( Text = "rel user(id: int, name: text) key(1).\nrel post(author: user).\nrel source(id: int, name: text).\npost(user(Id, Name)) <- source(Id, Name), user(Id, Name).\n",
        lower_text(existing_target_json, Text, [], _,
                   lowered(_, _, _, _, Levels, _, _, _)),
        member(levelstmt(post/1, _, Inserts, _, _, _), Levels),
        member(Sql, Inserts),
        sub_atom(Sql, _, _, _, 'b1."__id"'),
        \+ sub_atom(Sql, _, _, _, 'json_object') )).

check(direct_target_edge_trigger_projects_joined_identity,
      ( Text = "rel user(id: int, name: text) key(1).\nrel post(author: user) key(1).\npost(user(Id, Name)) <+ user(Id, Name).\n",
        lower_text(direct_target_trigger, Text, [], _,
                   lowered(_, _, _, Edges, _, _, _, _)),
        member(edgestmt(post/1, user/2, _, _, ProjectSql, _, _, _),
               Edges),
        sub_atom(ProjectSql, _, _, _, 'b0."__id"'),
        sub_atom(ProjectSql, _, _, _, 'FROM "user" b0'),
        \+ sub_atom(ProjectSql, _, _, _, 'json_object') )).

check(missing_target_constructor_currently_has_no_existence_join,
      ( Text = "rel user(id: int, name: text) key(1).\nrel post(author: user).\nrel source(id: int, name: text).\npost(user(Id, Name)) <- source(Id, Name).\n",
        lower_text(missing_target, Text, [], _,
                   lowered(_, _, _, _, Levels, _, _, _)),
        member(levelstmt(post/1, _, Inserts, _, _, _), Levels),
        member(Sql, Inserts),
        sub_atom(Sql, _, _, _, 'FROM "source"'),
        \+ sub_atom(Sql, _, _, _, 'FROM "user"'),
        \+ sub_atom(Sql, _, _, _, 'JOIN "user"') )).

check(runtime_intern_and_lookup_use_every_target_column_not_the_key,
      ( program_text(Text),
        plan_text(runtime_full_row, Text, [],
                  plan(_, prog(Decls, _), _, _, _, _)),
        struct_type_plans(Decls,
                          [structtype(user, [id, name], [none, none],
                                      InternSql, LookupSql)]),
        sub_atom(InternSql, _, _, _, '"id", "name"'),
        sub_atom(LookupSql, _, _, _, 'json_array("id", "name")') )).

check(same_key_conflicting_non_key_field_has_no_lookup_row,
      ( program_text(Text),
        plan_text(conflict_lookup, Text, [],
                  plan(_, prog(Decls, _), _, _, _, _)),
        struct_type_plans(Decls,
                          [structtype(user, _, _, InternSql, LookupSql)]),
        sub_atom(InternSql, 0, _, _, 'INSERT OR IGNORE'),
        sub_atom(LookupSql, _, _, _, 'json_array("id", "name")'),
        \+ sub_atom(LookupSql, _, _, _, 'WHERE "id" IN') )).

check(key_only_constructor_is_not_a_current_relation_term,
      ( Text = "rel user(id: int, name: text) key(1).\nrel post(author: user).\nrel source(id: int).\npost(user(Id)) <- source(Id).\n",
        lower_text(key_only_constructor, Text, [], _,
                   lowered(_, _, _, _, Levels, _, _, _)),
        member(levelstmt(post/1, _, Inserts, _, _, _), Levels),
        member(Sql, Inserts),
        sub_atom(Sql, _, _, _, 'json_object'),
        sub_atom(Sql, _, _, _, '''user''') )).

check(boot_parent_reference_still_asks_removed_semantic_column,
      ( program_text(Text),
        Initial = [user(1, alice),
                   post(obj([id-1, name-alice]))],
        lower_text(boot_parent, Text, Initial,
                   plan(_, prog(Decls, _), RelPlans, _, _, _),
                   lowered(_, Ddl, _, _, Levels, _, _, _)),
        boot_statements(Decls, RelPlans, Initial, Levels, Boot),
        member(bootstmt(BootSql, _), Boot),
        sub_atom(BootSql, _, _, _, '"__semantic"'),
        atomic_list_concat(Ddl, '\n', DdlSql),
        \+ sub_atom(DdlSql, _, _, _, '"__semantic"') )).
