erase_type_apply_transport(Decls0, Decls) :-
    exclude(type_apply_transport_decl, Decls0, Decls).

type_apply_transport_decl(compiler_type_apply_request_rows(_)).
type_apply_transport_decl(compiler_type_apply_request(_)).
type_apply_transport_decl(compiler_derived_type_demand(_)).
type_apply_transport_decl(compiler_derived_type_application(_)).
type_apply_transport_decl(compiler_derived_relation_request_rows(_)).
type_apply_transport_decl(compiler_derived_member_role(_, _, _, _)).

type_apply_requests(SourceDecls, Decls, Requests) :-
    request_rows(Decls, RequestRows),
    findall(Request,
            ( member(type_apply_request(Application), RequestRows),
              Application = application(Constructor, _),
              \+ frozen_type_application(Decls, Application, Constructor),
              type_apply_request_decl(SourceDecls, Constructor, Application,
                                      Request),
              ground(Request) ),
            TypeApplyRequests),
    findall(Carrier,
            ( member(compiler_derived_relation_request_rows(Shapes), Decls),
              member(Shape, Shapes),
              derived_relation_shape_carrier(SourceDecls, Shape, Carrier) ),
            DerivedCarriers),
    append(TypeApplyRequests, DerivedCarriers, Requests0),
    % Shape members arrive in declared position order. Preserve that order while
    % removing duplicate carrier rows so concat-like derivations stay positional.
    list_to_set(Requests0, Requests).

request_rows(Decls, Rows) :-
    member(compiler_type_apply_request_rows(Rows), Decls),
    !.
request_rows(_, []).

type_apply_request_decl(Decls, Constructor, Application,
                        compiler_derived_type_demand(Application)) :-
    semantic_type_constructor_term(Decls, Constructor, Name),
    compiler_derived_constructor(Decls, Name, Expected),
    !,
    Application = application(_, Arguments),
    length(Arguments, Found),
    ( Expected =:= Found
    -> true
    ;  throw(unsupported_construct(
           type_apply_arity_mismatch(Constructor, Expected, Found))) ).
type_apply_request_decl(Decls, Constructor, Application,
                        compiler_type_apply_request(Type)) :-
    type_apply_request_type(Decls, Constructor, Application, Type).

derived_relation_shape_carrier(Decls,
        derived_relation_shape(Application, Constructor, _, _, _, _),
        compiler_derived_type_application(Type)) :-
    semantic_type_term(Decls, Application, Type),
    Application = application(Constructor, _).
derived_relation_shape_carrier(Decls,
        derived_relation_shape(Application, named(Module, relation, _), _,
                               _, _, _),
        semantic_decl_module(relation, GeneratedName, Module)) :-
    semantic_type_term(Decls, Application, Type),
    canonical_type_name(Type, GeneratedName).
derived_relation_shape_carrier(Decls,
        derived_relation_shape(Application, _, _, _, Members, _),
        type_decl(GeneratedName, Specs)) :-
    semantic_type_term(Decls, Application, Type),
    canonical_type_name(Type, GeneratedName),
    maplist(derived_relation_member_spec(Decls), Members, Specs).
derived_relation_shape_carrier(Decls,
        derived_relation_shape(Application, _, _, Count, Members, _),
        col_type(GeneratedName/Count, Name, MemberType)) :-
    semantic_type_term(Decls, Application, Type),
    canonical_type_name(Type, GeneratedName),
    member(member(_, Name, MemberTypeId), Members),
    semantic_type_term(Decls, MemberTypeId, MemberType).
derived_relation_shape_carrier(Decls,
        derived_relation_shape(Application, _, _, Count, Members, Roles),
        keyed(GeneratedName/Count, KeyPositions)) :-
    findall(Position,
            member(role(Position, key, _), Roles),
            KeyPositions0),
    sort(KeyPositions0, KeyPositions),
    KeyPositions \== [],
    forall(member(Position, KeyPositions),
           memberchk(member(Position, _, _), Members)),
    semantic_type_term(Decls, Application, Type),
    canonical_type_name(Type, GeneratedName).
derived_relation_shape_carrier(Decls,
        derived_relation_shape(Application, _, _, _, Members, Roles),
        compiler_derived_member_role(GeneratedName, Position, Role,
                                     Argument)) :-
    member(role(Position, Role, Argument), Roles),
    memberchk(member(Position, _, _), Members),
    semantic_type_term(Decls, Application, Type),
    canonical_type_name(Type, GeneratedName).

derived_relation_member_spec(Decls, member(_, Name, MemberTypeId),
                             col(Name, MemberType)) :-
    semantic_type_term(Decls, MemberTypeId, MemberType).

frozen_type_application(Decls, Application, Constructor) :-
    member(semantic_type_rows(Rows), Decls),
    memberchk(application(Application, Constructor), Rows).

type_apply_request_type(Decls, Constructor, Application, Type) :-
    semantic_type_constructor_term(Decls, Constructor, Name),
    ( member(rel_template(Segments, Parameters, _), Decls),
      atomic_list_concat(Segments, '__', Name)
    -> Application = application(_, Arguments),
       length(Parameters, Expected), length(Arguments, Found),
       ( Expected =:= Found -> true
       ; throw(unsupported_construct(
             type_apply_arity_mismatch(Constructor, Expected, Found))) ),
       semantic_type_term(Decls, Application, Type)
    ; builtin_type_constructor(Name)
    -> Application = application(_, Arguments),
       length(Arguments, Found),
       ( Found =:= 1 -> semantic_type_term(Decls, Application, Type)
       ; throw(unsupported_construct(
             type_apply_arity_mismatch(Constructor, 1, Found))) )
    ; member(rel_template_enum(Segments, Parameters, _), Decls),
      atomic_list_concat(Segments, '__', Name)
    -> Application = application(_, Arguments),
       length(Parameters, Expected), length(Arguments, Found),
       ( Expected =:= Found -> semantic_type_term(Decls, Application, Type)
       ; throw(unsupported_construct(
             type_apply_arity_mismatch(Constructor, Expected, Found))) )
    ; throw(unsupported_construct(type_apply_unknown_constructor(Constructor)))
    ).
