:- begin_tests(userland_type_operators).

:- use_module('../../../compile', [program_plan/2]).
:- use_module('../../../use_resolve', [expand_uses/8]).

operator_fixture_path(Name, Path) :-
    predicate_property(
        plunit_userland_type_operators:operator_fixture_path(_, _),
        file(ThisFile)),
    file_directory_name(ThisFile, TestDir),
    atomic_list_concat(['../../../../dl/fixtures/', Name], Relative),
    absolute_file_name(Relative, Path,
                       [relative_to(TestDir), access(read)]).

operator_fixture_plan(Name, Plan) :-
    operator_fixture_path(Name, Fixture),
    once((
        expand_uses(Fixture, [], [], _, Program, _, Bindings, Findings),
        Findings == [],
        compile:dl6_seeded_form(Program, Initial, Seeded),
        program_plan(fixture(userland_type_operators, Seeded, Initial, [], [])-
                     Bindings, Plan)
    )).

plan_compiler_closure(Plan, Closure) :-
    Plan = plan(_, prog(Decls, _), _, _, _, _, _, _, _),
    ( memberchk(compiler_type_metadata(_, Closure, _), Decls)
    -> true
    ; memberchk(compiler_type_metadata(_, Closure), Decls)
    ).

test(all_operators_share_one_userland_compiler_closure) :-
    operator_fixture_plan('0_userland-type-operators.dl6', Plan),
    plan_compiler_closure(Plan, Closure),
    Plan = plan(_, prog(Decls, RuntimeRules), _, RelPlans, _, _, _, _, _),
    User = named(Module, relation, 'User'),
    UserPatch = named(Module, relation, 'UserPatch'),
    Base = named(Module, relation, 'Base'),
    Middle = named(Module, relation, 'Middle'),
    Leaf = named(Module, relation, 'Leaf'),
    Interface = named(Module, relation, 'Interface'),
    ParentInterface = named(Module, relation, 'ParentInterface'),
    RecursiveNode = named(Module, relation, 'RecursiveNode'),
    BlockedNode = named(Module, relation, 'BlockedNode'),
    Opaque = named(Module, relation, 'Opaque'),
    Partial = named(OperatorModule, relation, 'Partial'),
    Concat = named(OperatorModule, relation, concat),
    PartialApplication = application(Partial, [User]),
    ConcatApplication = application(Concat, [User, UserPatch]),
    OpaqueText = application(Opaque, [primitive(text)]),
    RecursiveList = application(named(local, relation, list), [RecursiveNode]),
    memberchk('Partial'(User, PartialApplication), Closure),
    memberchk(concat(User, UserPatch, ConcatApplication), Closure),
    memberchk(concat_output(ConcatApplication), Closure),
    memberchk(extends(Leaf, Middle), Closure),
    memberchk(extends(Leaf, Base), Closure),
    memberchk(impl(Leaf, Interface), Closure),
    memberchk(impl(Leaf, ParentInterface), Closure),
    memberchk(serializable(RecursiveNode), Closure),
    memberchk(serializable(RecursiveList), Closure),
    memberchk(serializable(primitive(text)), Closure),
    memberchk(serialization_blocked(OpaqueText), Closure),
    memberchk(serialization_blocked(BlockedNode), Closure),
    \+ memberchk(serializable(BlockedNode), Closure),
    memberchk(has_text_member(User), Closure),
    PartialGenerated = '__gen__Partial_User_9d7a703929b72789',
    ConcatGenerated = '__gen__concat_User_UserPatch_89b827c1054dd1da',
    memberchk(
        rel(PartialGenerated/2, _, set,
            [ col(id, declared(option(int)), int),
              col(name, declared(option(text)), int) ], none),
        RelPlans),
    memberchk(
        rel(ConcatGenerated/3, _, set,
            [ col(id, declared(int), int),
              col(name, declared(text), text),
              col(active, declared(bool), bool) ], key([1])),
        RelPlans),
    memberchk(semantic_type_rows(TypeRows), Decls),
    PartialGeneratedId = named(OperatorModule, relation, PartialGenerated),
    \+ member(member_role(member(PartialGeneratedId, _, _), key), TypeRows),
    memberchk(member_role(member(PartialGeneratedId, 1, id), optionalized),
              TypeRows),
    memberchk(member_role(member(PartialGeneratedId, 2, name), optionalized),
              TypeRows),
    RuntimeRules =
        [ '<-'('__opt_text_tag'(TextNoneId, none),
               '__opt_text_none'(TextNoneId)),
          '<-'('__opt_text_tag'(TextSomeId, some),
               '__opt_text_some'(TextSomeId, _)),
          '<-'('__opt_int_tag'(IntNoneId, none),
               '__opt_int_none'(IntNoneId)),
          '<-'('__opt_int_tag'(IntSomeId, some),
               '__opt_int_some'(IntSomeId, _)) ],
    forall(member(rel(Name/_, _, _, _, _), RelPlans),
           \+ memberchk(Name,
                        ['Partial', concat, extends, impl,
                         derive_serializable, serialization_candidate,
                         serialization_shape, serialization_constructor,
                         serialization_blocked, serializable,
                         has_text_member])).

test(concat_rejects_incompatible_member_names,
     [throws(unsupported_construct(
         derived_relation_request_name_conflict(_, [value, value])))]) :-
    operator_fixture_plan('1_userland-type-operators-conflict.dl6', _).

:- end_tests(userland_type_operators).
