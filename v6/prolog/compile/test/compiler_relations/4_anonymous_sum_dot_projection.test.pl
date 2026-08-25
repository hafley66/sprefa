:- begin_tests(anonymous_sum_dot_projection).

:- use_module('../../../0_dot_expand',
              [resolve_qualified_types/2, resolve_relation_paths/3]).
:- use_module('../../../1_expansion', [expand_program_with_bindings/4]).
:- use_module('../../../compile', [program_plan/2]).
:- use_module('../../parse_dl_dcg', [parse_dl/4]).
:- use_module('../../../use_resolve', [expand_uses/8]).

fixture_path(Path) :-
    predicate_property(
        plunit_anonymous_sum_dot_projection:fixture_path(_),
        file(ThisFile)),
    file_directory_name(ThisFile, TestDir),
    absolute_file_name(
        '../../../../dl/fixtures/3_anonymous-sum-dot-projection.dl6', Path,
        [relative_to(TestDir), access(read)]).

fixture_plan(Plan) :-
    fixture_path(Fixture),
    once((
        expand_uses(Fixture, [], [], _, Program, _, Bindings, Findings),
        Findings == [],
        compile:dl6_seeded_form(Program, Initial, Seeded),
        program_plan(fixture(anonymous_sum_dot_projection,
                             Seeded, Initial, [], [])-Bindings,
                     Plan)
    )).

parse_text(Source, Program, Bindings) :-
    string_codes(Source, Codes),
    parse_dl(Codes, Program, Bindings, []).

resolved_text(Source, prog(Decls, Rules)) :-
    parse_text(Source, Program, _),
    resolve_qualified_types(Program, prog(Decls, Rules0)),
    resolve_relation_paths(Decls, Rules0, Rules).

test(surface_type_and_rule_paths_resolve_to_generated_declarations) :-
    Source = "rel Event(payload: (created(value: int); deleted(reason: text))).\n\c
              rel Envelope(payload: Event.payload).\n\c
              rel CreatedReference(value: Event.payload.created).\n\c
              rel seen_created(id: int, value: int).\n\c
              seen_created(Id, Value) <- Event.payload.created(Id, Value).\n",
    resolved_text(Source, prog(Decls, Rules)),
    memberchk(type_path_alias(EnumName/0, ['Event', payload]), Decls),
    memberchk(type_path_alias(CreatedName/2,
                              ['Event', payload, created]), Decls),
    memberchk(col_type('Envelope'/1, payload, EnumName), Decls),
    memberchk(col_type('CreatedReference'/1, value, CreatedName), Decls),
    memberchk((seen_created(Id, Value) <- CreatedRow), Rules),
    CreatedRow =.. [CreatedName, Id, Value].

test(compiler_paths_and_userland_projection_share_the_anonymous_graph) :-
    fixture_plan(plan(_, prog(Decls, RuntimeRules), _, RelPlans,
                      _, _, _, _, _)),
    memberchk(semantic_type_rows(SemanticRows), Decls),
    memberchk(compiler_type_metadata(_, Closure), Decls),
    once(member(anonymous(Event, [payload], Shape), SemanticRows)),
    Shape = sum_type([variant(created, [field(value, int)]),
                      variant(deleted, [field(reason, text)])]),
    memberchk(derived_from(EventPayload, anonymous(Event, [payload], Shape)),
              SemanticRows),
    memberchk(member(_, EventPayload, _, created,
                     type_ref(declaration(Created))), SemanticRows),
    memberchk(seen_project(Event, payload, EventPayload), Closure),
    memberchk(seen_project(EventPayload, created, Created), Closure),
    memberchk(seen_path(anonymous(Event, [payload], Shape),
                        ['Event', payload]), Closure),
    memberchk(seen_path(EventPayload, ['Event', payload]), Closure),
    memberchk(seen_path(Created, ['Event', payload, created]), Closure),
    memberchk((seen_created(_, _) <- CreatedCall), RuntimeRules),
    Event = named(Module, relation, 'Event'),
    EventPayload = named(Module, enum, _),
    Created = named(Module, relation, CreatedName),
    functor(CreatedCall, CreatedName, 2),
    memberchk(type_decl(CreatedName, [col(id, int), col(value, int)]), Decls),
    memberchk(rel(CreatedName/2, _, _, _, _), RelPlans),
    \+ member(type_path_alias(_, _), Decls).

test(authored_nested_path_collision_is_deterministic,
     [throws(unsupported_construct(
                 mount_path_collision(
                     ['A', x], 'A__x',
                     '__anon_A_x_952ec73c907ddb82')))]) :-
    Source = "rel A(x: (left(); right())).\nrel A.x(value: int).\n",
    parse_text(Source, Program, _),
    resolve_qualified_types(Program, _).

test(unrelated_declarations_do_not_change_anonymous_path_targets) :-
    Base = "rel A(x: (left(); right())).\n",
    Extended = "rel unrelated(value: text).\n\c
                rel A(x: (left(); right())).\n",
    anonymous_path_targets(Base, BaseTargets),
    anonymous_path_targets(Extended, ExtendedTargets),
    BaseTargets \== [],
    BaseTargets == ExtendedTargets.

anonymous_path_targets(Source, Targets) :-
    resolved_text(Source, prog(Decls, _)),
    findall(Path-Ref,
            ( member(type_path_alias(Ref, Path), Decls),
              Path = ['A', x | _] ),
            Targets0),
    sort(Targets0, Targets).

test(generic_and_recursive_owners_produce_finite_deterministic_paths) :-
    Source = "rel Box(T)(value: T).\n\c
              rel Use(box: Box((left(value: int); right()))).\n\c
              rel node(next: option(node), event: (leaf(); branch(child: option(node)))).\n\c
              rel seen_path(Node: type, Path: semantic).\n\c
              seen_path(Node, Path) <- type.path(Node, Path).\n",
    parse_text(Source, Program, Bindings),
    once(expand_program_with_bindings(Program, Bindings, prog(Decls, _), _)),
    memberchk(compiler_type_metadata(_, Closure), Decls),
    findall(Path, member(seen_path(_, Path), Closure), Paths0),
    sort(Paths0, Paths),
    memberchk([node, event], Paths),
    memberchk([node, event, branch], Paths),
    once(( member([GenericOwner, value], Paths),
           atom(GenericOwner),
           sub_atom(GenericOwner, 0, _, _, '__gen__Box') )),
    \+ member([node, next, next | _], Paths).

:- end_tests(anonymous_sum_dot_projection).
