% The compiler plane closes before enum, storage, and runtime planning.  Its
% declarations/rules disappear from the executable program while the semantic
% member/type-relation rows remain available to catalog and typegen consumers.
elaborate_and_erase_compiler_relations(Decls0, Rules0, Bindings, Decls, Rules) :-
    partition_compiler_program(Decls0, Rules0, CompilerDecls0, RuntimeDecls,
                               RuntimeRules),
    CompilerDecls0 = compiler_relations(Relations, CompilerRules0),
    ( Relations == []
    -> erase_annotation_transport(RuntimeDecls, Decls, _),
       Rules = RuntimeRules
    ;  type_relation_rows(Decls0, MetadataRows),
       elaborate_compiler_rules(Decls0, Bindings, CompilerRules0,
                                CompilerRules, SeedRows),
       compiler_type_source_rows(Decls0, Relations, TypeSourceRows),
       compiler_annotation_site_rows(Decls0, Relations, SiteRows),
       append([SeedRows, TypeSourceRows, SiteRows], CompilerSeedRows),
       evaluate_compiler_relations(compiler_relations(Relations, CompilerRules),
                                   CompilerSeedRows, ClosureRows),
       compiler_type_apply_requests(CompilerRules, ClosureRows, RequestRows),
       compiler_derived_relation_shapes(ClosureRows, DerivedShapes),
       erase_annotation_transport(RuntimeDecls, RuntimeDecls1, AnnotationEvidence),
       ( AnnotationEvidence == []
       -> Metadata = compiler_type_metadata(MetadataRows, ClosureRows)
       ;  Metadata = compiler_type_metadata(MetadataRows, ClosureRows,
                                            AnnotationEvidence)
       ),
       append(RuntimeDecls1, [Metadata,
                              compiler_type_apply_request_rows(RequestRows),
                              compiler_derived_relation_request_rows(DerivedShapes)],
              Decls),
       Rules = RuntimeRules
    ).

compiler_derived_relation_shapes(Closure, Shapes) :-
    findall(Application,
            compiler_derived_request_application(Closure, Application),
            Applications0),
    sort(Applications0, Applications),
    maplist(validate_compiler_derived_relation_shape(Closure), Applications,
            Shapes).

compiler_derived_request_application(Closure, Application) :-
    member(derived_relation_request(Application, _, _, _), Closure).
compiler_derived_request_application(Closure, Application) :-
    member(derived_member_request(Application, _, _, _), Closure).
compiler_derived_request_application(Closure, Application) :-
    member(derived_member_role_request(Application, _, _, _), Closure).

validate_compiler_derived_relation_shape(Closure, Application,
        derived_relation_shape(Application, Constructor, Arguments, Count,
                               Members, Roles)) :-
    findall(header(HeaderConstructor, HeaderArguments, HeaderCount),
            member(derived_relation_request(Application, HeaderConstructor,
                                            HeaderArguments, HeaderCount),
                   Closure),
            Headers0),
    sort(Headers0, Headers),
    validate_derived_request_header(Application, Headers,
                                    Constructor, Arguments, Count),
    findall(member(Position, Name, Type),
            member(derived_member_request(Application, Position, Name, Type),
                   Closure),
            Members0),
    sort(Members0, Members),
    validate_derived_request_members(Application, Count, Members),
    findall(role(Position, Role, Argument),
            member(derived_member_role_request(Application, Position, Role,
                                               Argument), Closure),
            Roles0),
    sort(Roles0, Roles),
    validate_derived_request_roles(Application, Members, Roles),
    ( memberchk(type_requested(Application, Constructor, Arguments), Closure)
    -> true
    ;  throw(unsupported_construct(
           derived_relation_request_without_demand(Application)))
    ).

validate_derived_request_header(Application, [], _, _, _) :-
    throw(unsupported_construct(derived_relation_request_missing_header(
                                    Application))).
validate_derived_request_header(Application,
                                [header(Constructor, Arguments, Count)],
                                Constructor, Arguments, Count) :-
    !,
    ( Application == application(Constructor, Arguments)
    -> true
    ;  throw(unsupported_construct(derived_relation_request_identity(
           Application, Constructor, Arguments)))
    ),
    ( integer(Count), Count >= 0
    -> true
    ;  throw(unsupported_construct(derived_relation_request_member_count(
           Application, Count)))
    ).
validate_derived_request_header(Application, Headers, _, _, _) :-
    throw(unsupported_construct(derived_relation_request_header_conflict(
                                    Application, Headers))).

validate_derived_request_members(Application, Count, Members) :-
    length(Members, Found),
    ( Found =:= Count
    -> true
    ;  throw(unsupported_construct(derived_relation_request_incomplete(
           Application, expected(Count), found(Found))))
    ),
    findall(Position, member(member(Position, _, _), Members), Positions),
    derived_request_expected_positions(Count, ExpectedPositions),
    ( Positions == ExpectedPositions
    -> true
    ;  throw(unsupported_construct(derived_relation_request_positions(
           Application, expected(ExpectedPositions), found(Positions))))
    ),
    findall(Name, member(member(_, Name, _), Members), Names),
    sort(Names, UniqueNames),
    ( same_length(Names, UniqueNames)
    -> true
    ;  throw(unsupported_construct(derived_relation_request_name_conflict(
           Application, Names)))
    ),
    forall(member(member(_, _, Type), Members),
           ( valid_semantic_type_id(Type)
           -> true
           ;  throw(unsupported_construct(derived_relation_request_type(
                  Application, Type))) )).

derived_request_expected_positions(0, []) :- !.
derived_request_expected_positions(Count, Positions) :-
    numlist(1, Count, Positions).

validate_derived_request_roles(Application, Members, Roles) :-
    findall(Position-Role,
            member(role(Position, Role, _), Roles), RoleKeys0),
    sort(RoleKeys0, RoleKeys),
    forall(member(Position-Role, RoleKeys),
           validate_derived_request_role_key(Application, Position, Role,
                                             Roles)),
    forall(member(role(Position, Role, Argument), Roles),
           ( memberchk(member(Position, _, _), Members),
             atom(Role),
             ground(Argument)
           -> true
           ;  throw(unsupported_construct(derived_relation_request_role(
                  Application, Position, Role, Argument))) )).

validate_derived_request_role_key(Application, Position, Role, Roles) :-
    findall(Argument,
            member(role(Position, Role, Argument), Roles), Arguments),
    ( Arguments = [_]
    -> true
    ;  throw(unsupported_construct(derived_relation_request_role_conflict(
           Application, Position, Role, Arguments)))
    ).

valid_semantic_type_id(primitive(Name)) :- atom(Name).
valid_semantic_type_id(named(Module, Kind, Name)) :-
    ground(Module), atom(Kind), atom(Name).
valid_semantic_type_id(application(Constructor, Arguments)) :-
    valid_semantic_type_id(Constructor),
    is_list(Arguments),
    maplist(valid_semantic_type_id, Arguments).
valid_semantic_type_id(parameter(Owner, Position, Name)) :-
    ground(Owner), integer(Position), atom(Name).
valid_semantic_type_id(anonymous_placeholder(Shape)) :- ground(Shape).

erase_annotation_transport(Decls0, Decls, Evidence) :-
    ( select(compiler_annotation_evidence(Evidence0), Decls0, WithoutEvidence)
    -> Evidence = Evidence0
    ;  Evidence = [], WithoutEvidence = Decls0
    ),
    exclude(annotation_transport_decl, WithoutEvidence, Decls).

annotation_transport_decl(compiler_annotation_requests(_)).
annotation_transport_decl(compiler_annotation_evidence(_)).

elaborate_compiler_rules(Decls, Bindings, Rules0, Rules, SeedRows) :-
    findall(Row,
            ( member(Rule, Rules0), compiler_fact_rule(Rule, Head),
              elaborate_compiler_fact_atom(Decls, Bindings, Head, Row) ),
            SeedRows0),
    sort(SeedRows0, SeedRows),
    findall(Rule,
            ( member(Rule0, Rules0), \+ compiler_fact_rule(Rule0, _),
              elaborate_compiler_rule(Decls, Bindings, Rule0, Rule) ),
            Rules).

compiler_fact_rule((Head <- true), Head) :- !.
compiler_fact_rule(Head, Head) :-
    compound(Head),
    Head \= (_ <- _),
    Head \= (_ <+ _).

elaborate_compiler_rule(Decls, Bindings, (Head <- Body0), (Head1 <- Body)) :-
    !,
    elaborate_compiler_head_atom(Decls, Bindings, Head, Head1, HeadGoals),
    elaborate_compiler_body(Decls, Bindings, Body0, Body1),
    append_compiler_body_goals(Body1, HeadGoals, Body).
elaborate_compiler_rule(Decls, Bindings, (Head <+ Body0), (Head1 <+ Body)) :-
    !,
    elaborate_compiler_head_atom(Decls, Bindings, Head, Head1, HeadGoals),
    elaborate_compiler_body(Decls, Bindings, Body0, Body1),
    append_compiler_body_goals(Body1, HeadGoals, Body).

elaborate_compiler_head_atom(Decls, Bindings, Atom0, Atom, Goals) :-
    Atom0 =.. [Name | Arguments0],
    length(Arguments0, Arity),
    compiler_relation_signature(Decls, Name/Arity, Types),
    elaborate_compiler_head_arguments(Decls, Bindings, Types, Arguments0,
                                      Arguments, Goals),
    Atom =.. [Name | Arguments].

elaborate_compiler_head_arguments(_, _, [], [], [], []).
elaborate_compiler_head_arguments(Decls, Bindings,
                                  [Domain0 | Domains], [Argument0 | Rest0],
                                  [Argument | Rest], Goals) :-
    compiler_argument_domain_or_self(Domain0, Domain),
    elaborate_compiler_head_argument(Decls, Bindings, Domain, Argument0,
                                     Argument, ArgumentGoals),
    elaborate_compiler_head_arguments(Decls, Bindings, Domains, Rest0, Rest,
                                      RestGoals),
    append(ArgumentGoals, RestGoals, Goals).

compiler_argument_domain_or_self(Domain0, Domain) :-
    ( compiler_argument_domain(Domain0, Unwrapped)
    -> Domain = Unwrapped
    ;  Domain = Domain0
    ).

elaborate_compiler_head_argument(Decls, Bindings, type, Argument0, Argument,
                                 Goals) :-
    compound(Argument0),
    Argument0 =.. [ConstructorName | Arguments0],
    compiler_type_constructor(Decls, ConstructorName, ExpectedArity),
    !,
    length(Arguments0, FoundArity),
    ( ExpectedArity =:= FoundArity
    -> true
    ;  semantic_decl_id(Decls, relation, ConstructorName, ConstructorId),
       throw(unsupported_construct(
           type_apply_arity_mismatch(ConstructorId, ExpectedArity,
                                     FoundArity)))
    ),
    elaborate_compiler_type_term_arguments(Decls, Bindings, Arguments0,
                                           Arguments, NestedGoals),
    semantic_type_constructor_id(Decls, ConstructorName, ConstructorId),
    append(NestedGoals,
           [type_apply(ConstructorId, Arguments, Argument)], Goals).
elaborate_compiler_head_argument(Decls, Bindings, Domain, Argument0, Argument,
                                 []) :-
    elaborate_compiler_argument(Decls, Bindings, Domain, Argument0, Argument).

elaborate_compiler_type_term_arguments(_, _, [], [], []).
elaborate_compiler_type_term_arguments(Decls, Bindings,
                                       [Argument0 | Rest0], [Argument | Rest],
                                       Goals) :-
    elaborate_compiler_head_argument(Decls, Bindings, type, Argument0,
                                     Argument, ArgumentGoals),
    elaborate_compiler_type_term_arguments(Decls, Bindings, Rest0, Rest,
                                           RestGoals),
    append(ArgumentGoals, RestGoals, Goals).

append_compiler_body_goals(Body, [], Body) :- !.
append_compiler_body_goals(true, Goals, Body) :- !,
    goals_body_conjunction(Goals, Body).
append_compiler_body_goals(Body0, Goals, (Body0, Body)) :-
    goals_body_conjunction(Goals, Body).

elaborate_compiler_body(_, _, true, true) :- !.
elaborate_compiler_body(Decls, Bindings, (Left0, Right0), (Left, Right)) :- !,
    elaborate_compiler_body(Decls, Bindings, Left0, Left),
    elaborate_compiler_body(Decls, Bindings, Right0, Right).
elaborate_compiler_body(Decls, Bindings, Atom0, Atom) :-
    elaborate_compiler_atom(Decls, Bindings, Atom0, Atom).

elaborate_compiler_atom(Decls, Bindings, Atom0, Atom) :-
    Atom0 =.. [Name | Arguments0],
    length(Arguments0, Arity),
    compiler_relation_signature(Decls, Name/Arity, Types),
    maplist(elaborate_compiler_argument(Decls, Bindings), Types, Arguments0, Arguments),
    Atom =.. [Name | Arguments].

% Fact variables may be source type names captured by the parser bindings.
% Rule variables remain evaluator joins and are preserved by
% elaborate_compiler_argument/5.
elaborate_compiler_fact_atom(Decls, Bindings, Atom0, Atom) :-
    Atom0 =.. [Name | Arguments0],
    length(Arguments0, Arity),
    compiler_relation_signature(Decls, Name/Arity, Types),
    maplist(elaborate_compiler_fact_argument(Decls, Bindings), Types,
            Arguments0, Arguments),
    Atom =.. [Name | Arguments].

elaborate_compiler_fact_argument(Decls, Bindings, Domain0, Argument,
                                 Elaborated) :-
    compiler_argument_domain(Domain0, Domain),
    Domain \== Domain0,
    !,
    elaborate_compiler_fact_argument(Decls, Bindings, Domain, Argument,
                                     Elaborated).
elaborate_compiler_fact_argument(Decls, Bindings, type, Argument, Elaborated) :-
    compiler_type_source_term(Decls, Bindings, Argument, Type),
    compiler_declared_type_term(Decls, Type),
    !,
    semantic_type_id(Decls, Type, Elaborated).
elaborate_compiler_fact_argument(_, _, type, Argument, _) :-
    throw(unsupported_construct(compiler_relation_type_unknown(Argument))).
elaborate_compiler_fact_argument(Decls, Bindings, Domain, Argument,
                                 Elaborated) :-
    elaborate_compiler_argument(Decls, Bindings, Domain, Argument, Elaborated).

compiler_relation_signature(_, Ref, Types) :-
    compiler_type_source_signature(Ref, Types), !.
compiler_relation_signature(Decls, Ref, Types) :-
    Ref = Name/_,
    annotation_relation_ref(Decls, Name, Ref),
    findall(Type, member(col_type(Ref, _, Type), Decls), Types).

compiler_type_source_signature(type_decl/4,
                               [semantic, text, text, text]).
compiler_type_source_signature(type_member/5,
                               [semantic, semantic, int, text, semantic]).
compiler_type_source_signature(type_member_role/3,
                               [semantic, text, text]).
compiler_type_source_signature(type_application/2,
                               [semantic, semantic]).
compiler_type_source_signature(type_argument/4,
                               [semantic, semantic, int, semantic]).
compiler_type_source_signature(type_application_site/4,
                               [relation_value, semantic, semantic, semantic]).
compiler_type_source_signature(type_apply/3, [type, semantic_type_ids, type]).
compiler_type_source_signature(type_requested/3,
                               [type, type, semantic_type_ids]).
compiler_type_source_signature(type_field/5,
                               [semantic, type, int, text, type]).
compiler_type_source_signature(type_field_count/2, [type, int]).
compiler_type_source_signature(derived_relation_request/4,
                               [type, type, semantic_type_ids, int]).
compiler_type_source_signature(derived_member_request/4,
                               [type, int, text, type]).
compiler_type_source_signature(derived_member_role_request/4,
                               [type, int, text, semantic]).

elaborate_compiler_argument(Decls, Bindings, Domain0, Argument, Elaborated) :-
    compiler_argument_domain(Domain0, Domain),
    Domain \== Domain0,
    !,
    elaborate_compiler_argument(Decls, Bindings, Domain, Argument, Elaborated).
elaborate_compiler_argument(Decls, Bindings, type, Argument, Elaborated) :-
    var(Argument),
    source_variable_name(Bindings, Argument, Name),
    compiler_declared_type_term(Decls, Name),
    !,
    semantic_type_id(Decls, Name, Elaborated).
elaborate_compiler_argument(_, _, _, Argument, Argument) :- var(Argument), !.
elaborate_compiler_argument(Decls, Bindings, type, Argument, Elaborated) :-
    compiler_type_source_term(Decls, Bindings, Argument, Type),
    compiler_declared_type_term(Decls, Type),
    !,
    semantic_type_id(Decls, Type, Elaborated).
elaborate_compiler_argument(_, _, type, Argument, _) :-
    throw(unsupported_construct(compiler_relation_type_unknown(Argument))).
elaborate_compiler_argument(_, _, int, Argument, Argument) :- integer(Argument), !.
elaborate_compiler_argument(_, _, text, Argument, Argument) :- atom(Argument), !.
elaborate_compiler_argument(_, _, bool, Argument, Argument) :-
    memberchk(Argument, [true, false]), !.
elaborate_compiler_argument(_, _, bool, bool_lit(Argument), Argument) :-
    memberchk(Argument, [true, false]), !.
elaborate_compiler_argument(_, _, float, Argument, Argument) :- float(Argument), !.
elaborate_compiler_argument(_, _, float, float_lit(Argument), Argument) :- float(Argument), !.
elaborate_compiler_argument(_, _, semantic, Argument, Argument) :- ground(Argument), !.
elaborate_compiler_argument(Decls, Bindings, semantic_type_ids, Arguments0,
                            Arguments) :-
    is_list(Arguments0),
    !,
    maplist(elaborate_compiler_semantic_type_id(Decls, Bindings), Arguments0,
            Arguments).
elaborate_compiler_argument(Decls, Bindings, relation_value, Argument,
                            Elaborated) :-
    compiler_relation_value(Decls, Bindings, Argument, Elaborated),
    !.
elaborate_compiler_argument(_, _, Type, Argument, _) :-
    throw(unsupported_construct(compiler_relation_argument_type(Type, Argument))).

elaborate_compiler_semantic_type_id(_, _, Value, Value) :- var(Value), !.
elaborate_compiler_semantic_type_id(Decls, Bindings, Value0, Value) :-
    compiler_type_source_term(Decls, Bindings, Value0, Type),
    compiler_declared_type_term(Decls, Type),
    semantic_type_id(Decls, Type, Value).

compiler_relation_value(_, _, Value0, Value) :-
    Value0 = relation_value(_, _),
    !,
    Value = Value0.
compiler_relation_value(Decls, Bindings, Value0, Value) :-
    compound(Value0),
    Value0 =.. [Name | Arguments0],
    length(Arguments0, Arity),
    Ref = Name/Arity,
    findall(Type, member(col_type(Ref, _, Type), Decls), Types),
    Types \== [],
    maplist(elaborate_compiler_argument(Decls, Bindings), Types,
            Arguments0, Arguments),
    semantic_decl_id(Decls, relation, Name, RelationId),
    Row =.. [Name | Arguments],
    Value = relation_value(RelationId, Row).

% key(type) carries relation-key role metadata on the declaration plane. A
% compiler relation receives the same semantic type value as an unwrapped
% type column; the key role remains in schema/type-relation metadata.
compiler_argument_domain(key(type), type).

%! compiler_type_source_rows(+Decls, +Relations, -Rows) is det.
%  Project only referenced compiler-source views from the frozen canonical
%  graph. These rows enter the compiler evaluator as seeds and never enter
%  runtime declarations or storage.
compiler_type_source_rows(Decls, Relations, Rows) :-
    ( member(semantic_type_rows(SemanticRows), Decls) -> true
    ; generic_type_ir(Decls, SemanticRows) ),
    findall(Row,
            ( member(compiler_relation(Ref, _, _), Relations),
              compiler_type_source_row(SemanticRows, Ref, Row) ),
            CanonicalRows),
    findall(Row,
            ( member(compiler_relation(Ref, _, _), Relations),
              compiler_type_transport_source_row(Decls, Ref, Row) ),
            TransportRows),
    append(CanonicalRows, TransportRows, Rows0),
    sort(Rows0, Rows).

compiler_type_source_row(Rows, type_decl/4,
                         type_decl(Id, Name, Kind, Phase)) :-
    member(declaration(Id, _, Name, Kind, Phase), Rows).
compiler_type_source_row(Rows, type_member/5, Row) :-
    member(Row0, Rows),
    Row0 = member(MemberId, OwnerId, Position, Name, TypeRef),
    Row = type_member(MemberId, OwnerId, Position, Name, TypeRef).
compiler_type_source_row(Rows, type_member_role/3,
                         type_member_role(MemberId, Role, Argument)) :-
    member(member_role(MemberId, RoleTerm), Rows),
    compiler_member_role_parts(RoleTerm, Role, Argument).
compiler_type_source_row(Rows, type_application/2,
                         type_application(ApplicationId, ConstructorId)) :-
    member(application(ApplicationId, ConstructorId), Rows).
compiler_type_source_row(Rows, type_argument/4,
                         type_argument(ArgumentId, ApplicationId, Position,
                                       TypeRef)) :-
    member(argument(ArgumentId, ApplicationId, Position, TypeRef), Rows).
compiler_type_source_row(Rows, type_requested/3,
                         type_requested(ApplicationId, ConstructorId,
                                        Arguments)) :-
    member(application(ApplicationId, ConstructorId), Rows),
    ApplicationId = application(ConstructorId, Arguments).
compiler_type_source_row(Rows, type_field/5,
                         type_field(MemberId, OwnerId, Position, Name,
                                    ValueTypeId)) :-
    member(member(MemberId, MaterializedOwner, Position, Name, TypeRef), Rows),
    compiler_field_owner(Rows, MaterializedOwner, OwnerId),
    compiler_field_value_type(TypeRef, ValueTypeId).
compiler_type_source_row(Rows, type_field_count/2,
                         type_field_count(OwnerId, Count)) :-
    member(declaration(MaterializedOwner, _, _, relation, _), Rows),
    compiler_field_owner(Rows, MaterializedOwner, OwnerId),
    findall(MemberId,
            member(member(MemberId, MaterializedOwner, _, _, _), Rows),
            MemberIds),
    length(MemberIds, Count).

compiler_type_transport_source_row(Decls, type_requested/3,
                                   type_requested(Application, Constructor,
                                                  Arguments)) :-
    member(compiler_derived_type_demand(Application), Decls),
    Application = application(Constructor, Arguments).

compiler_field_owner(_, Owner, Owner).
compiler_field_owner(Rows, MaterializedOwner, Application) :-
    member(derived_from(MaterializedOwner, Application), Rows).

compiler_field_value_type(type_ref(primitive(Name)), primitive(Name)) :- !.
compiler_field_value_type(type_ref(declaration(Id)), Id) :- !.
compiler_field_value_type(type_ref(application(Id)), Id) :- !.
compiler_field_value_type(type_ref(Id), Id).

compiler_annotation_site_rows(Decls, Relations, Rows) :-
    ( memberchk(compiler_relation(type_application_site/4, _, _), Relations),
      memberchk(compiler_annotation_evidence(Evidence), Decls)
    -> findall(type_application_site(Application, Owner, Member, Position),
               ( member(annotation_evidence(Member, Site, Ordinal, _, _, _,
                                            AnnotationRow), Evidence),
                 AnnotationRow =.. [Name | _],
                 semantic_decl_id(Decls, relation, Name, RelationId),
                 Application = relation_value(RelationId, AnnotationRow),
                 Member = member(Owner, _, _),
                 Position = site(Site, Ordinal) ),
               Rows0),
       sort(Rows0, Rows)
    ;  Rows = []
    ).

compiler_member_role_parts(anonymous_owner(Path), anonymous_owner, Path) :- !.
compiler_member_role_parts(RoleTerm, Role, Argument) :-
    compound(RoleTerm),
    RoleTerm =.. [Role, Argument],
    !.
compiler_member_role_parts(Role, Role, '').

compiler_type_source_term(Decls, Bindings, Variable, Type) :-
    var(Variable),
    !,
    source_variable_name(Bindings, Variable, Type),
    compiler_declared_type(Decls, Type).
compiler_type_source_term(_, _, Type, Type) :- atom(Type), !.
compiler_type_source_term(Decls, Bindings, Term0, Term) :-
    compound(Term0),
    Term0 =.. [Name | Arguments0],
    maplist(compiler_type_source_term(Decls, Bindings), Arguments0, Arguments),
    Term =.. [Name | Arguments].

source_variable_name(Bindings, Variable, Name) :-
    member(Binding, Bindings),
    ( Binding = (Name=Existing) ; Binding = Name-Existing ),
    Existing == Variable,
    !.

compiler_declared_type(Decls, Name) :- compiler_declared_type_term(Decls, Name).

compiler_declared_type_term(_, Type) :- atom(Type), semantic_primitive(Type), !.
compiler_declared_type_term(_, Type) :- builtin_type_constructor(Type), !.
compiler_declared_type_term(Decls, Type) :-
    atom(Type),
    ( member(type_decl(Type, _), Decls)
    ; member(col_type(Type/_, _, _), Decls)
    ; member(rel_template(Segments, _, _), Decls), atomic_list_concat(Segments, '__', Type)
    ; member(enum_decl(Type, _), Decls)
    ; member(rel_template_enum(Segments, _, _), Decls), atomic_list_concat(Segments, '__', Type)
    ), !.
compiler_declared_type_term(Decls, Type) :-
    atom(Type),
    member(semantic_type_rows(Rows), Decls),
    member(declaration(_, _, Type, relation, compile_time), Rows), !.
compiler_declared_type_term(Decls, Type) :-
    compound(Type),
    Type =.. [Constructor | Arguments],
    compiler_type_constructor(Decls, Constructor, Arity),
    length(Arguments, Arity),
    maplist(compiler_declared_type_term(Decls), Arguments).

compiler_type_constructor(_, option, 1).
compiler_type_constructor(_, json_list, 1).
compiler_type_constructor(_, list, 1).
compiler_type_constructor(Decls, Constructor, Arity) :-
    member(rel_template(Segments, _, _), Decls),
    atomic_list_concat(Segments, '__', Constructor),
    member(rel_template(Segments, Parameters, _), Decls),
    length(Parameters, Arity).
compiler_type_constructor(Decls, Constructor, Arity) :-
    member(rel_template_enum(Segments, Parameters, _), Decls),
    atomic_list_concat(Segments, '__', Constructor),
    length(Parameters, Arity).
compiler_type_constructor(Decls, Constructor, Arity) :-
    compiler_derived_constructor(Decls, Constructor, Arity).
compiler_type_constructor(Decls, Constructor, Arity) :-
    member(semantic_type_rows(Rows), Decls),
    member(declaration(ConstructorId, _, Constructor, relation, compile_time),
           Rows),
    findall(Position,
            member(parameter(_, ConstructorId, Position, _), Rows),
            Positions),
    length(Positions, Arity).

compiler_derived_constructor(Decls, Name, Arity) :-
    member(col_type(Name/FullArity, return, type), Decls),
    findall(Column-Type,
            member(col_type(Name/FullArity, Column, Type), Decls),
            Columns),
    append(Inputs, [return-type], Columns),
    forall(member(_-InputType, Inputs), compiler_constructor_input(InputType)),
    length(Inputs, Arity).

compiler_constructor_input(type).
compiler_constructor_input(key(type)).
