:- begin_tests(userland_type_projection).

:- use_module('../../../0_generic_expand',
              [expand_generic_program_with_bindings/3]).
:- use_module('../../../0_compiler_relations',
              [partition_compiler_relations/3]).
:- use_module('../../../compile', [program_plan/2]).
:- use_module('../../parse_dl_dcg', [parse_dl/4, parse_dl_file/4]).
:- use_module('../../../use_resolve', [expand_uses/8]).

projection_fixture_path(Path) :-
    predicate_property(
        plunit_userland_type_projection:projection_fixture_path(_),
        file(ThisFile)),
    file_directory_name(ThisFile, TestDir),
    absolute_file_name(
        '../../../../dl/fixtures/2_userland-type-projection.dl6', Path,
        [relative_to(TestDir), access(read)]).

projection_library_path(Path) :-
    predicate_property(
        plunit_userland_type_projection:projection_library_path(_),
        file(ThisFile)),
    file_directory_name(ThisFile, TestDir),
    absolute_file_name('../../../../dl/type/1_projection.dl6', Path,
                       [relative_to(TestDir), access(read)]).

projection_fixture_plan(Plan) :-
    projection_fixture_path(Fixture),
    once((
        expand_uses(Fixture, [], [], _, Program, _, Bindings, Findings),
        Findings == [],
        compile:dl6_seeded_form(Program, Initial, Seeded),
        program_plan(fixture(userland_type_projection, Seeded, Initial, [], [])-
                     Bindings, Plan)
    )).

runtime_rule_head_ref('<-'(Head, _), Name/Arity) :-
    functor(Head, Name, Arity).

test(the_dl6_library_declares_the_projection_key) :-
    projection_library_path(Library),
    once(parse_dl_file(Library, prog(Decls, _), _, [])),
    partition_compiler_relations(
        Decls, compiler_relations(Relations, []), _),
    memberchk(compiler_relation(type__project/3, 3, [1, 2]), Relations).

test(the_projection_relation_requires_an_explicit_declaration,
     [throws(unsupported_construct(unresolvable_path([type, project])))]) :-
    Source = "rel seen(Owner: type, Name: text, Target: type).\n\c
              seen(Owner, Name, Target) <- type.project(Owner, Name, Target).\n",
    string_codes(Source, Codes),
    once(parse_dl(Codes, Program, Bindings, [])),
    expand_generic_program_with_bindings(Program, Bindings, _).

test(dl6_rules_derive_member_variant_and_nested_projections) :-
    projection_fixture_plan(
        plan(_, prog(Decls, RuntimeRules), _, RelPlans, _, _, _, _, _)),
    memberchk(compiler_type_metadata(_, Closure), Decls),
    Item = named(Module, relation, 'Item'),
    Address = named(Module, relation, 'Address'),
    Status = named(Module, enum, 'Status'),
    Audit = named(Module, relation, 'Item__Audit'),
    Same = named(Module, relation, 'Same'),
    SameChild = named(Module, relation, 'Same__child'),
    Inline = named(Module, relation, 'Inline'),
    OptionText = application(named(local, relation, option),
                             [primitive(text)]),
    memberchk(seen_project(Item, maybe, OptionText), Closure),
    memberchk(seen_project(Item, home, Address), Closure),
    memberchk(seen_project(Item, 'Audit', Audit), Closure),
    memberchk(seen_project(Status, ready, _), Closure),
    memberchk(seen_project(Status, failed, _), Closure),
    findall(Target,
            member(seen_project(Same, child, Target), Closure),
            [SameChild]),
    memberchk(seen_project(Inline, value, InlineEnum), Closure),
    InlineEnum = named(Module, enum, InlineEnumName),
    sub_atom(InlineEnumName, 0, _, _, '__anon_Inline_value_'),
    \+ member(seen_project(_, _, reference(_)), Closure),
    \+ member(col_type(type__project/3, _, _), Decls),
    \+ member(col_type(seen_project/3, _, _), Decls),
    findall(Ref,
            ( member(Rule, RuntimeRules),
              runtime_rule_head_ref(Rule, Ref) ),
            RuntimeRefs),
    \+ memberchk(type__project/3, RuntimeRefs),
    \+ memberchk(seen_project/3, RuntimeRefs),
    forall(member(rel(Name/_, _, _, _, _), RelPlans),
           \+ memberchk(Name, [type__project, seen_project])).

:- end_tests(userland_type_projection).
