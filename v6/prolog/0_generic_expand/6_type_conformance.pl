% The source syntax stays in the authored declaration list.  This plane is a
% compiler-only relational view over it and the normalized semantic rows.  It
% never contributes a `col_type/3`, kind, fact, or rule to the runtime program.
%
% Type signatures and lifetime:
%
%   compile_type_plane(+Decls, +NormalizedRows, -Plane) is det.
%   compile_type_query(+Plane, +Goal, -Proof) is semidet.
%
% A Plane exists from generic expansion's validation/fixpoint boundary through
% bound judgment.  `semantic_type_rows/1` retains existing declaration and
% accepted-obligation metadata for the catalog, while the view and its proof
% values are discarded before runtime lowering.
compile_type_plane(Decls, Rows, type_plane(Decls, Rows)).

validate_compile_type_plane(Plane) :-
    ( compile_type_query(Plane, duplicate_interface(Name), _)
    -> throw(unsupported_construct(interface_duplicate(Name)))
    ; true
    ).

% Compile-time relation declarations and source facts.
compile_type_relation(type_plane(_, Rows), interface, [Name, Arity]) :-
    member(declaration(InterfaceId, root, Name, interface, compile_time), Rows),
    findall(_, member(parameter(_, InterfaceId, _, _), Rows), Parameters),
    length(Parameters, Arity).
compile_type_relation(type_plane(Decls, _), named_type, [Name]) :-
    ( member(type_decl(Name, _), Decls)
    ; member(col_type(Name/_, _, _), Decls)
    ).
compile_type_relation(type_plane(Decls, _), field, [Name, Type]) :-
    named_type_columns(Decls, Name, ColumnTypes),
    member(Type, ColumnTypes).
compile_type_relation(type_plane(Decls, _), enum, [Name]) :-
    member(enum_decl(Name, _), Decls).
compile_type_relation(type_plane(Decls, _), enum_payload, [Name, Type]) :-
    member(enum_decl(Name, Variants), Decls),
    enum_payload_types(Variants, PayloadTypes),
    member(Type, PayloadTypes).

% One query boundary for interface diagnostics and structural conformance.
compile_type_query(type_plane(Decls, _), duplicate_interface(Name), duplicate) :-
    findall(InterfaceName, member(interface_decl(InterfaceName, _), Decls), Names),
    select(Name, Names, Rest),
    memberchk(Name, Rest),
    !.
compile_type_query(Plane, conforms(Type, interface_pattern(InterfaceName, Patterns)), Proof) :-
    compile_type_conformance(Plane, Type,
                             interface_pattern(InterfaceName, Patterns), [], Proof).
compile_type_query(Plane, conforms(Type, Interface), Proof) :-
    interface_application_parts(Interface, InterfaceName, Patterns),
    compile_type_conformance(Plane, Type, interface_pattern(InterfaceName, Patterns), [], Proof).

% A recursive revisit closes structural json_encodable proofs.
compile_type_conformance(_, Type, interface_pattern(json_encodable, []), Seen,
                         structural(Type)) :-
    atom(Type),
    memberchk(Type, Seen),
    !.
% The generated rules remain ordinary compile-time relation rules.  The
% evaluator below is deliberately thin: it only supplies recursion guards and
% the all-fields/all-payloads join needed by JsonEncodable's existing rule.
compile_type_conformance(_, Type, interface_pattern(json_encodable, []), _,
                         structural(Type)) :-
    scalar_element(Type),
    !.
compile_type_conformance(Plane, option(Type), interface_pattern(json_encodable, []), Seen,
                        structural(option(Type))) :-
    !,
    compile_type_conformance(Plane, Type, interface_pattern(json_encodable, []), Seen, _).
compile_type_conformance(Plane, json_list(Type), interface_pattern(json_encodable, []), Seen,
                        structural(json_list(Type))) :-
    !,
    compile_type_conformance(Plane, Type, interface_pattern(json_encodable, []), Seen, _).
compile_type_conformance(Plane, Name, interface_pattern(json_encodable, []), Seen, structural(Name)) :-
    atom(Name),
    \+ memberchk(Name, Seen),
    compile_type_relation(Plane, named_type, [Name]),
    findall(FieldType, compile_type_relation(Plane, field, [Name, FieldType]), FieldTypes),
    maplist(compile_type_conformance_with_seen(Plane, [Name | Seen]), FieldTypes),
    !.
compile_type_conformance(Plane, Name, interface_pattern(json_encodable, []), Seen, structural(Name)) :-
    atom(Name),
    \+ memberchk(Name, Seen),
    compile_type_relation(Plane, enum, [Name]),
    findall(PayloadType,
            compile_type_relation(Plane, enum_payload, [Name, PayloadType]),
            PayloadTypes),
    maplist(compile_type_conformance_with_seen(Plane, [Name | Seen]), PayloadTypes),
    !.
compile_type_conformance(Plane, Type, interface_pattern(json_encodable, []), Seen, structural(Type)) :-
    compound(Type),
    Plane = type_plane(Decls, _),
    generic_application_name(Decls, Type),
    \+ memberchk(Type, Seen),
    canonical_type_name(Type, ConcreteName),
    compile_type_conformance(Plane, ConcreteName,
                             interface_pattern(json_encodable, []),
                             [Type | Seen], _).

compile_type_conformance_with_seen(Plane, Seen, Type) :-
    compile_type_conformance(Plane, Type,
                             interface_pattern(json_encodable, []), Seen, _).

% Checked at the source terms as well as normalized rows: interface application
% arguments remain visible for arity validation and conformance matching.
validate_interface_applications(Decls) :-
    findall(Name-Arity,
            ( member(interface_decl(Name, Parameters), Decls),
              length(Parameters, Arity) ),
            Interfaces),
    forall(( member(Template, Decls),
             template_parameters(Template, Parameters),
             member(Parameter, Parameters),
             parameter_parts(Parameter, ParameterName, Constraints),
             member(Constraint, Constraints) ),
           ( validate_interface_application(Constraint, Interfaces),
             reject_repeated_subject_bound(ParameterName, Constraint),
             reject_nested_bound_wildcard(Constraint),
             reject_multiple_bound_wildcards(Constraint) )),
    forall(( source_generic_application(Decls, Application),
             contains_any(Application) ),
           ( functor(Application, Constructor, _),
             throw(unsupported_construct(
                       interface_wildcard_in_concrete_application(Constructor))) )).

validate_interface_application(Application, Interfaces) :-
    ( compound(Application)
    -> functor(Application, Name, Arity)
    ; Name = Application, Arity = 0 ),
    ( memberchk(Name-Expected, Interfaces)
    -> ( Arity =:= Expected
       -> true
       ; throw(unsupported_construct(
                   interface_arity(Name, Expected, Arity))) )
    ; throw(unsupported_construct(interface_unknown(Name))) ).

reject_repeated_subject_bound(ParameterName, Constraint) :-
    interface_application_parts(Constraint, InterfaceName, [ParameterName]),
    throw(unsupported_construct(
              repeated_subject_interface_bound(ParameterName, InterfaceName))).
reject_repeated_subject_bound(_, _).

reject_nested_bound_wildcard(Constraint) :-
    interface_application_parts(Constraint, InterfaceName, Arguments),
    member(Argument, Arguments),
    compound(Argument),
    contains_any(Argument),
    throw(unsupported_construct(
              interface_nested_wildcard(InterfaceName, Argument))).
reject_nested_bound_wildcard(_).

reject_multiple_bound_wildcards(Constraint) :-
    interface_application_parts(Constraint, InterfaceName, Arguments),
    include(==(any), Arguments, Wildcards),
    length(Wildcards, Count),
    Count > 1,
    throw(unsupported_construct(
              interface_multiple_wildcards(InterfaceName))).
reject_multiple_bound_wildcards(_).

contains_any(any) :- !.
contains_any(Term) :-
    compound(Term),
    compound_name_arguments(Term, _, Arguments),
    member(Argument, Arguments),
    contains_any(Argument).

parameter_parts(type_parameter(Name, Constraints), Name, Constraints) :- !.
parameter_parts(Name, Name, []).

parameter_id_for(Owner, Parameters, Name, Id) :-
    nth1(Ordinal, Parameters, Parameter),
    parameter_parts(Parameter, Name, _),
    param_id(Owner, Ordinal, Name, Id), !.

generic_owner_parameters(Decls, Name, Parameters) :-
    member(rel_template(Segments, Parameters, _), Decls),
    atomic_list_concat(Segments, '__', Name).

interface_application_name(Application, Name) :-
    ( compound(Application) -> functor(Application, Name, _)
    ; Name = Application ).

interface_application_parts(Application, Name, []) :-
    atom(Application), !,
    Name = Application.
interface_application_parts(Application, Name, Arguments) :-
    compound(Application),
    Application =.. [Name | Arguments].

ref_name(Name/_, Name) :- !.
ref_name(Name, Name).

semantic_decl_id(Decls, Kind, Name, Id) :-
    nb_current(generic_semantic_id_cache,
               cache(DeclCache0, TypeCache, OwnerCache)),
    !,
    (   get_assoc(Kind-Name, DeclCache0, Id)
    ->  true
    ;   semantic_decl_id_uncached(Decls, Kind, Name, Id),
        put_assoc(Kind-Name, DeclCache0, Id, DeclCache),
        nb_setval(generic_semantic_id_cache,
                  cache(DeclCache, TypeCache, OwnerCache))
    ).
semantic_decl_id(Decls, Kind, Name, Id) :-
    semantic_decl_id_uncached(Decls, Kind, Name, Id).

semantic_decl_id_uncached(Decls, Kind, Name, Id) :-
    (   member(semantic_decl_module(Kind, Name, ModuleHash), Decls)
    ->  true
    ;   semantic_row_decl_module(Decls, Kind, Name, ModuleHash)
    ->  true
    ;   generated_decl_module(Decls, Kind, Name, ModuleHash)
    ->  true
    ;   ModuleHash = local
    ),
    decl_id(ModuleHash, Kind, Name, Id).

semantic_row_decl_module(Decls, Kind, Name, ModuleHash) :-
    member(semantic_type_rows(Rows), Decls),
    member(declaration(named(ModuleHash, Kind, Name), _, Name, Kind, _), Rows).

generated_decl_module(Decls, relation, Name, ModuleHash) :-
    source_generic_application(Decls, Application),
    canonical_type_name(Application, Name),
    Application =.. [Constructor | _],
    member(semantic_decl_module(relation, Constructor, ModuleHash), Decls),
    !.
generated_decl_module(Decls, enum, Name, ModuleHash) :-
    enum_template_application(Decls, Application),
    canonical_type_name(Application, Name),
    Application =.. [Constructor | _],
    member(semantic_decl_module(enum, Constructor, ModuleHash), Decls),
    !.

enum_template_application(Decls, Application) :-
    source_type_application(Decls, Application),
    generic_enum_constructor_name(Decls, Name),
    functor(Application, Name, _).
generated_decl_module(Decls, relation, Name, ModuleHash) :-
    source_generic_application(Decls, Application),
    list_flavor_suffix(Application, Suffix),
    flavor_ref(Application, Suffix, Name/_),
    Application =.. [Constructor | _],
    member(semantic_decl_module(relation, Constructor, ModuleHash), Decls),
    !.

semantic_application_id(Decls, Constructor, Arguments, Id) :-
    maplist(semantic_application_argument_id(Decls), Arguments, ArgumentIds),
    app_id(Constructor, ArgumentIds, Id).

semantic_application_argument_id(_, any, any_pattern) :- !.
semantic_application_argument_id(Decls, Type, Id) :- semantic_type_id(Decls, Type, Id).

semantic_type_id(_, Type, Id) :-
    atom(Type),
    semantic_primitive(Type),
    !,
    primitive_id(Type, Id).
semantic_type_id(Decls, annotated_type(Type, _), Id) :-
    !,
    semantic_type_id(Decls, Type, Id).
semantic_type_id(_, Type, anonymous_placeholder(Type)) :-
    anonymous_type_term(Type),
    !.
semantic_type_id(Decls, Type, Id) :-
    atom(Type),
    !,
    semantic_named_type_id(Decls, Type, Id).
semantic_type_id(Decls, Type, Id) :-
    Type =.. [Name | Arguments],
    semantic_type_constructor_id(Decls, Name, Constructor),
    semantic_application_id(Decls, Constructor, Arguments, Id).

semantic_primitive(Type) :- scalar_element(Type).
semantic_primitive(bytes).

anonymous_type_term(product_type(_)).
anonymous_type_term(sum_type(_)).
anonymous_type_term(arrow_type(_, _)).
anonymous_type_term(anonymous_product(_, _)).
anonymous_type_term(anonymous_sum(_, _)).

semantic_named_type_id(Decls, Name, Id) :-
    member(semantic_decl_module(enum, Name, ModuleHash), Decls),
    !,
    decl_id(ModuleHash, enum, Name, Id).
semantic_named_type_id(Decls, Name, Id) :- semantic_decl_id(Decls, relation, Name, Id).
