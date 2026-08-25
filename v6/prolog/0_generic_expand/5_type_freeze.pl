%! freeze_type_rows(+ExpandedDecls, -FrozenDecls) is det.
% Merge generated carriers into the existing semantic identity graph without
% replacing source-level wrapper members by physical storage endpoints.
freeze_type_rows(Decls0, Decls) :-
    semantic_rows_in_decls(Decls0, ExistingRows0),
    ExistingRows = ExistingRows0,
    normalized_type_rows(Decls0, RebuiltRows),
    validate_type_row_identities(ExistingRows),
    validate_type_row_identities(RebuiltRows),
    merge_frozen_type_rows(ExistingRows, RebuiltRows, Rows),
    validate_type_application_closure(Rows),
    validate_nested_type_path_targets(Decls0, Rows),
    replace_semantic_type_rows(Decls0, Rows, Decls).

semantic_rows_in_decls(Decls, Rows) :-
    findall(TheseRows, member(semantic_type_rows(TheseRows), Decls), Nested),
    append(Nested, Rows).

replace_semantic_type_rows(Decls0, Rows, Decls) :-
    replace_semantic_type_rows(Decls0, Rows, false, Decls1, Seen),
    (   Seen == true
    ->  Decls = Decls1
    ;   Rows == []
    ->  Decls = Decls1
    ;   append(Decls1, [semantic_type_rows(Rows)], Decls)
    ).

replace_semantic_type_rows([], _, Seen, [], Seen).
replace_semantic_type_rows([semantic_type_rows(_) | Rest], Rows, false,
                           [semantic_type_rows(Rows) | More], Seen) :-
    !,
    replace_semantic_type_rows(Rest, Rows, true, More, Seen).
replace_semantic_type_rows([semantic_type_rows(_) | Rest], Rows, true,
                           More, Seen) :-
    !,
    replace_semantic_type_rows(Rest, Rows, true, More, Seen).
replace_semantic_type_rows([Decl | Rest], Rows, Seen0, [Decl | More], Seen) :-
    replace_semantic_type_rows(Rest, Rows, Seen0, More, Seen).

merge_frozen_type_rows(ExistingRows, RebuiltRows, Rows) :-
    existing_row_identities(ExistingRows, Identities),
    include(row_missing_from_identities(Identities), RebuiltRows, MissingRows),
    append(ExistingRows, MissingRows, Unsorted),
    sort(Unsorted, Rows),
    validate_type_row_identities(Rows).

% One identity pass over the existing rows: the scan this replaces recomputed
% every existing row's identity once per rebuilt row.
existing_row_identities(ExistingRows, Identities) :-
    findall(Identity,
            ( member(Row, ExistingRows),
              canonical_type_row_identity(Row, Identity) ),
            Unsorted),
    sort(Unsorted, Identities).

% \+/1 undoes memberchk/2's bindings, so the probe stays a unification test.
row_missing_from_identities(Identities, Row) :-
    canonical_type_row_identity(Row, Identity),
    \+ memberchk(Identity, Identities).

validate_type_row_identities(Rows) :-
    findall(Identity-Row,
            ( member(Row, Rows), canonical_type_row_identity(Row, Identity) ),
            Pairs),
    keysort(Pairs, Ordered),
    validate_type_row_identity_runs(Ordered).

validate_type_row_identity_runs([]).
validate_type_row_identity_runs([Identity-Row | Rest]) :-
    take_type_row_identity_run(Rest, Identity, [Row], Run, Remaining),
    validate_type_row_identity_run(Identity, Run),
    validate_type_row_identity_runs(Remaining).

take_type_row_identity_run([Identity-Row | Rest], Identity, Acc, Run, Remaining) :-
    !,
    take_type_row_identity_run(Rest, Identity, [Row | Acc], Run, Remaining).
take_type_row_identity_run(Remaining, _, Run, Run, Remaining).

validate_type_row_identity_run(_, [_]) :- !.
validate_type_row_identity_run(_, [Row | Rest]) :-
    forall(member(Other, Rest), Row == Other),
    !.
validate_type_row_identity_run(Identity, _) :-
    duplicate_type_row_subject(Identity, Subject),
    throw(unsupported_construct(canonical_type_row_duplicate(Subject))).

duplicate_type_row_subject(member(MemberId), MemberId) :- !.
duplicate_type_row_subject(Identity, Identity).

canonical_type_row_identity(declaration(Id, _, _, _, _), declaration(Id)) :- !.
canonical_type_row_identity(parameter(Id, _, _, _), parameter(Id)) :- !.
canonical_type_row_identity(member(Id, _, _, _, _), member(Id)) :- !.
canonical_type_row_identity(member_role(Id, Role), member_role(Id, Role)) :- !.
canonical_type_row_identity(application(Id, _), application(Id)) :- !.
canonical_type_row_identity(argument(Id, _, _, _), argument(Id)) :- !.
canonical_type_row_identity(constraint(Id, _, _), constraint(Id)) :- !.
canonical_type_row_identity(constraint(Id, _, _, _), constraint(Id)) :- !.
canonical_type_row_identity(derived_from(Id, Source), derived_from(Id, Source)) :- !.
canonical_type_row_identity(origin(Id, Source), origin(Id, Source)) :- !.
canonical_type_row_identity(anonymous(Owner, Path, _), anonymous(Owner, Path)) :- !.
canonical_type_row_identity(Row, Row).

validate_type_application_closure(Rows) :-
    forall(member(member(MemberId, _, _, _, type_ref(application(AppId))), Rows),
           ( member(application(AppId, ConstructorId), Rows)
           -> application_constructor_resolves(ConstructorId, Rows)
           ;  throw(unsupported_construct(
                         canonical_type_application_missing(MemberId, AppId)))
           )),
    forall(member(application(AppId, ConstructorId), Rows),
           validate_type_application_arguments(AppId, ConstructorId, Rows)).

application_constructor_resolves(ConstructorId, Rows) :-
    ConstructorId = named(_, relation, Name),
    ( member(declaration(ConstructorId, _, Name, relation, _), Rows)
    ; builtin_type_constructor(Name)
    ), !.
application_constructor_resolves(ConstructorId, Rows) :-
    ConstructorId = named(_, enum, Name),
    member(declaration(ConstructorId, _, Name, enum, _), Rows), !.
application_constructor_resolves(ConstructorId, _) :-
    throw(unsupported_construct(
              canonical_type_application_unknown_constructor(ConstructorId))).

builtin_type_constructor(option).
builtin_type_constructor(list).
builtin_type_constructor(json_list).
builtin_type_constructor(id).
builtin_type_constructor(list_entity_dense_sequence).
builtin_type_constructor(list_interned_set).
builtin_type_constructor(list_entity_linked_sequence).

validate_type_application_arguments(AppId, ConstructorId, Rows) :-
    AppId = application(_, ExpectedArgs),
    length(ExpectedArgs, ExpectedArity),
    findall(Ordinal-ArgumentType,
            member(argument(_, application(ConstructorId, ExpectedArgs), Ordinal,
                            ArgumentType), Rows),
            Found0),
    sort(Found0, Found),
    findall(Ordinal-ExpectedId,
            nth1(Ordinal, ExpectedArgs, ExpectedId),
            Expected),
    ( length(Found, ExpectedArity),
      application_argument_rows_match(Expected, Found)
    -> true
    ;  throw(unsupported_construct(
                 canonical_type_application_arguments(AppId,
                                                      expected(Expected),
                                                      found(Found))))
    ).

application_argument_rows_match([], []).
application_argument_rows_match([Ordinal-Expected | ExpectedRows],
                                [Ordinal-Found | FoundRows]) :-
    application_argument_row_matches(Expected, Found),
    application_argument_rows_match(ExpectedRows, FoundRows).

application_argument_row_matches(primitive(Name), type_atom(Name)) :- !.
application_argument_row_matches(named(_, _, Name), type_atom(Name)) :- !.
application_argument_row_matches(Id, type_declaration(Id)) :-
    Id = named(_, _, _), !.
application_argument_row_matches(Id, type_application(Id)) :-
    Id = application(_, _), !.
application_argument_row_matches(parameter(_, _, Name), type_atom(Name)) :- !.
application_argument_row_matches(anonymous_placeholder(Shape),
                                 type_named(Shape)) :- !.
application_argument_row_matches(any_pattern, any_pattern).

normalized_declaration_row(Decls, declaration(Id, root, Name, relation, materialized)) :-
    member(type_decl(Name, _), Decls),
    semantic_decl_id(Decls, relation, Name, Id).
normalized_declaration_row(Decls, declaration(Id, root, Name, relation, compile_time)) :-
    member(rel_template(Segments, _, _), Decls),
    atomic_list_concat(Segments, '__', Name),
    semantic_decl_id(Decls, relation, Name, Id).
normalized_declaration_row(Decls, declaration(Id, root, Name, interface, compile_time)) :-
    member(interface_decl(Name, _), Decls),
    semantic_decl_id(Decls, interface, Name, Id).
normalized_declaration_row(Decls, declaration(Id, root, Name, enum, compile_time)) :-
    member(enum_decl(Name, _), Decls),
    semantic_decl_id(Decls, enum, Name, Id).
normalized_declaration_row(Decls, declaration(Id, root, Name, enum, compile_time)) :-
    member(rel_template_enum(Segments, _, _), Decls),
    atomic_list_concat(Segments, '__', Name),
    semantic_decl_id(Decls, enum, Name, Id).
normalized_declaration_row(Decls, declaration(Id, root, ConcreteName, relation, materialized)) :-
    source_generic_application(Decls, Application),
    canonical_type_name(Application, ConcreteName),
    semantic_decl_id(Decls, relation, ConcreteName, Id).
normalized_declaration_row(Decls, declaration(Id, root, OwnerName, relation, materialized)) :-
    plain_relation_specs(Decls, OwnerName, _),
    semantic_decl_id(Decls, relation, OwnerName, Id).
normalized_declaration_row(Decls, declaration(Id, root, Name, relation,
                                              materialized)) :-
    member(kind(Name/0, _), Decls),
    semantic_decl_id(Decls, relation, Name, Id).

normalized_parameter_row(Decls, parameter(Id, Owner, Ordinal, Name)) :-
    generic_owner_parameters(Decls, OwnerName, Parameters),
    semantic_decl_id(Decls, relation, OwnerName, Owner),
    nth1(Ordinal, Parameters, Parameter0),
    parameter_parts(Parameter0, Name, _),
    param_id(Owner, Ordinal, Name, Id).
normalized_parameter_row(Decls, parameter(Id, Owner, Ordinal, Name)) :-
    member(interface_decl(OwnerName, Parameters), Decls),
    semantic_decl_id(Decls, interface, OwnerName, Owner),
    nth1(Ordinal, Parameters, Parameter0),
    parameter_parts(Parameter0, Name, _),
    param_id(Owner, Ordinal, Name, Id).

normalized_member_row(Decls, member(Id, Owner, Ordinal, Name, Type)) :-
    member(rel_template(Segments, Parameters, Specs), Decls),
    atomic_list_concat(Segments, '__', OwnerName),
    semantic_decl_id(Decls, relation, OwnerName, Owner),
    nth1(Ordinal, Specs, column(Name, Type0)),
    normalized_type(Decls, Owner, Parameters, Type0, Type),
    member_id(Owner, Ordinal, Name, Id).
normalized_member_row(Decls, member(Id, Owner, Ordinal, Name, Type)) :-
    member(type_decl(OwnerName, Specs), Decls),
    semantic_decl_id(Decls, relation, OwnerName, Owner),
    nth1(Ordinal, Specs, col(Name, Type0)),
    normalized_type(Decls, Owner, [], Type0, Type),
    member_id(Owner, Ordinal, Name, Id).
normalized_member_row(Decls, member(Id, Owner, Ordinal, Name, Type)) :-
    plain_relation_specs(Decls, OwnerName, Specs),
    semantic_decl_id(Decls, relation, OwnerName, Owner),
    nth1(Ordinal, Specs, col(Name, Type0)),
    normalized_type(Decls, Owner, [], Type0, Type),
    member_id(Owner, Ordinal, Name, Id).

% A few compiler-owned and conformance programs provide only col_type/3
% entries.  Keep the normalized member graph available for those programs as
% well as for parser-produced type_decl/2 entries.
% One grouping pass over the declarations, rather than a full scan per owner
% name. The scan form below ran findall/3 and memberchk/2 over all 1246 pokeapi
% declarations once for each of its 212 owners and measured 17.5 ms of a 500 ms
% compile. keysort/2 is stable, so a group keeps the declaration order the
% findall produced, and its keys arrive in the sorted order setof/3 produced.
% An owner name that is not an atom cannot be a group key without changing which
% declarations the scan's unification reaches, so that case keeps the scan.
plain_relation_specs(Decls, OwnerName, Specs) :-
    (   plain_relation_spec_groups(Decls, Groups)
    ->  member(OwnerName-Specs, Groups)
    ;   plain_relation_specs_scan(Decls, OwnerName, Specs)
    ).

plain_relation_spec_groups(Decls, Groups) :-
    findall(Name-col(Column, Type),
            member(col_type(Name/_Arity, Column, Type), Decls),
            Pairs),
    forall(member(Name-_, Pairs), atom(Name)),
    keysort(Pairs, Sorted),
    group_pairs_by_key(Sorted, Grouped),
    findall(Declared, member(type_decl(Declared, _), Decls), TypeDeclared),
    exclude(owner_has_type_decl(TypeDeclared), Grouped, Groups).

owner_has_type_decl(TypeDeclared, OwnerName-_) :-
    memberchk(OwnerName, TypeDeclared).

plain_relation_specs_scan(Decls, OwnerName, Specs) :-
    setof(Name,
          Arity^Column^Type^member(col_type(Name/Arity, Column, Type), Decls),
          OwnerNames),
    member(OwnerName, OwnerNames),
    \+ memberchk(type_decl(OwnerName, _), Decls),
    findall(Name-Type,
            member(col_type(OwnerName/_, Name, Type), Decls), Pairs0),
    Pairs0 \== [],
    maplist(pair_col, Pairs0, Specs).

pair_col(Name-Type, col(Name, Type)).

% Two syntactic routes can reach the same member identity. Exact duplicate
% rows collapse. Divergent descriptions are rejected at the semantic boundary
% rather than depending on clause order to select one.
first_member_row_per_id(Rows, Kept) :-
    empty_assoc(Seen),
    first_member_row_per_id(Rows, Seen, [], Reversed),
    reverse(Reversed, Kept).

first_member_row_per_id([], _, Kept, Kept).
first_member_row_per_id([Row | Rest], Seen0, Kept0, Kept) :-
    Row = member(Id, _, _, _, _),
    (   get_assoc(Id, Seen0, Existing)
    ->  ( Existing == Row
        -> Seen = Seen0,
           Kept1 = Kept0
        ;  throw(unsupported_construct(canonical_type_row_duplicate(Id)) )
        )
    ;   put_assoc(Id, Seen0, Row, Seen),
        Kept1 = [Row | Kept0]
    ),
    first_member_row_per_id(Rest, Seen, Kept1, Kept).

normalized_constraint_row(Decls, constraint(Id, ParameterId, InterfaceId)) :-
    generic_owner_parameters(Decls, OwnerName, Parameters),
    semantic_decl_id(Decls, relation, OwnerName, Owner),
    nth1(Ordinal, Parameters, Parameter0),
    parameter_parts(Parameter0, Name, Constraints),
    param_id(Owner, Ordinal, Name, ParameterId),
    member(Constraint, Constraints),
    interface_application_parts(Constraint, InterfaceName, []),
    semantic_decl_id(Decls, interface, InterfaceName, InterfaceId),
    constraint_id(ParameterId, InterfaceId, Id).

normalized_constraint_row(Decls, constraint(Id, ParameterId, InterfaceId)) :-
    member(interface_decl(OwnerName, Parameters), Decls),
    semantic_decl_id(Decls, interface, OwnerName, Owner),
    nth1(Ordinal, Parameters, Parameter0),
    parameter_parts(Parameter0, Name, Constraints),
    param_id(Owner, Ordinal, Name, ParameterId),
    member(Constraint, Constraints),
    interface_application_parts(Constraint, InterfaceName, []),
    semantic_decl_id(Decls, interface, InterfaceName, InterfaceId),
    constraint_id(ParameterId, InterfaceId, Id).

normalized_constraint_row(Decls, constraint(Id, ParameterId, InterfaceId, Patterns)) :-
    generic_owner_parameters(Decls, OwnerName, Parameters),
    semantic_decl_id(Decls, relation, OwnerName, Owner),
    nth1(Ordinal, Parameters, Parameter0),
    parameter_parts(Parameter0, Name, Constraints),
    param_id(Owner, Ordinal, Name, ParameterId),
    member(Constraint, Constraints),
    interface_application_parts(Constraint, InterfaceName, Patterns),
    Patterns \== [],
    semantic_decl_id(Decls, interface, InterfaceName, InterfaceId),
    semantic_application_id(Decls, InterfaceId, Patterns, InterfaceApplicationId),
    constraint_id(ParameterId, InterfaceApplicationId, Id).

normalized_constraint_row(Decls, constraint(Id, ParameterId, InterfaceId, Patterns)) :-
    member(interface_decl(OwnerName, Parameters), Decls),
    semantic_decl_id(Decls, interface, OwnerName, Owner),
    nth1(Ordinal, Parameters, Parameter0),
    parameter_parts(Parameter0, Name, Constraints),
    param_id(Owner, Ordinal, Name, ParameterId),
    member(Constraint, Constraints),
    interface_application_parts(Constraint, InterfaceName, Patterns),
    Patterns \== [],
    semantic_decl_id(Decls, interface, InterfaceName, InterfaceId),
    semantic_application_id(Decls, InterfaceId, Patterns, InterfaceApplicationId),
    constraint_id(ParameterId, InterfaceApplicationId, Id).

member_constraint_row(Rows, ParameterId, InterfaceId, []) :-
    member(constraint(_, ParameterId, InterfaceId), Rows).
member_constraint_row(Rows, ParameterId, InterfaceId, Patterns) :-
    member(constraint(_, ParameterId, InterfaceId, Patterns), Rows).

normalized_application_rows(Decls, Rows) :-
    findall(Application, semantic_application_source(Decls, Application), Found),
    sort(Found, Applications),
    maplist(normalized_application_row(Decls), Applications, Nested),
    append(Nested, Rows).

semantic_application_source(Decls, Application) :-
    source_type_application(Decls, Application),
    semantic_application_constructor(Decls, Application).

source_type_application(Decls, Application) :-
    member(type_decl(_, Specs), Decls),
    member(col(_, Type), Specs),
    sub_term(Application, Type),
    compound(Application).
source_type_application(Decls, Application) :-
    member(rel_template(_, _, Specs), Decls),
    member(column(_, Type), Specs),
    sub_term(Application, Type),
    compound(Application).
source_type_application(Decls, Application) :-
    member(rel_template_enum(_, _, Variants), Decls),
    enum_payload_type(Variants, Type),
    sub_term(Application, Type),
    compound(Application).
source_type_application(Decls, Application) :-
    member(col_type(_, _, Type), Decls),
    sub_term(Application, Type),
    compound(Application).
source_type_application(Decls, Application) :-
    member(compiler_derived_type_application(Application), Decls).

semantic_application_constructor(Decls, Application) :-
    generic_application_name(Decls, Application),
    !.
semantic_application_constructor(Decls, Application) :-
    compound(Application),
    functor(Application, Name, Arity),
    compiler_derived_constructor(Decls, Name, Arity),
    !.
semantic_application_constructor(_, Application) :-
    Application =.. [Name | Arguments],
    builtin_type_application(Name, Arguments).

builtin_type_application(option, [_]).
builtin_type_application(list, [_]).
builtin_type_application(json_list, [_]).
builtin_type_application(id, [_]).
builtin_type_application(list_entity_dense_sequence, [_]).
builtin_type_application(list_interned_set, [_]).
builtin_type_application(list_entity_linked_sequence, [_]).

source_generic_application(Decls, Application) :-
    memberchk(rel_template(_, _, _), Decls),
    source_generic_application_(Decls, Application).

source_generic_application_(Decls, Application) :-
    member(type_decl(_, Specs), Decls),
    member(col(_, Type), Specs),
    sub_term(Application, Type),
    generic_application_name(Decls, Application).
source_generic_application_(Decls, Application) :-
    member(col_type(_, _, Type), Decls),
    sub_term(Application, Type),
    generic_application_name(Decls, Application).

generic_application_name(Decls, Application) :-
    compound(Application),
    functor(Application, Name, Arity),
    member(rel_template(Segments, Parameters, _), Decls),
    atomic_list_concat(Segments, '__', Name),
    length(Parameters, Arity).
generic_application_name(Decls, Application) :-
    compound(Application),
    functor(Application, Name, Arity),
    member(rel_template_enum(Segments, Parameters, _), Decls),
    atomic_list_concat(Segments, '__', Name),
    length(Parameters, Arity).

normalized_application_row(Decls, Application, Rows) :-
    Application =.. [Name | Arguments],
    semantic_type_constructor_id(Decls, Name, Constructor),
    semantic_application_id(Decls, Constructor, Arguments, Id),
    ApplicationRow = application(Id, Constructor),
    findall(Row,
            ( nth1(Ordinal, Arguments, Type0),
              normalized_argument_type(Decls, Type0, Type),
              arg_id(Id, Ordinal, ArgumentId),
              Row = argument(ArgumentId, Id, Ordinal, Type) ),
            ArgumentRows),
    Rows = [ApplicationRow | ArgumentRows].

normalized_argument_type(_, Type, type_atom(Type)) :- atom(Type), !.
normalized_argument_type(_, Type, type_named(Type)) :-
    anonymous_type_term(Type),
    !.
normalized_argument_type(Decls, Type, type_application(Id)) :-
    compound(Type),
    Type =.. [Constructor | Arguments],
    semantic_decl_id(Decls, relation, Constructor, ConstructorId),
    semantic_application_id(Decls, ConstructorId, Arguments, Id).

normalized_derivation_rows(Decls, ApplicationRows, Rows) :-
    findall(derived_from(ConcreteId, ApplicationId),
            ( member(application(ApplicationId, ConstructorId), ApplicationRows),
              id_kind_name(ConstructorId, relation, ConstructorName),
              source_application_by_constructor(Decls, ConstructorName, Application),
              Application =.. [_ | Arguments],
              semantic_application_id(Decls, ConstructorId, Arguments, ApplicationId),
              canonical_type_name(Application, ConcreteName),
              semantic_decl_id(Decls, relation, ConcreteName, ConcreteId) ),
            Found),
    sort(Found, Rows).

source_application_by_constructor(Decls, Name, Application) :-
    member(col_type(_, _, Type), Decls),
    sub_term(Application, Type),
    compound(Application), functor(Application, Name, _).
source_application_by_constructor(Decls, Name, Application) :-
    member(type_decl(_, Specs), Decls),
    member(col(_, Type), Specs),
    sub_term(Application, Type),
    compound(Application), functor(Application, Name, _).
source_application_by_constructor(Decls, Name, Application) :-
    member(compiler_derived_type_application(Application), Decls),
    compound(Application), functor(Application, Name, _).

normalized_type(Decls, Owner, Parameters, Type0, type_ref(Type)) :-
    Type0 = key(Inner),
    !,
    normalized_type(Decls, Owner, Parameters, Inner, type_ref(Type)).
normalized_type(Decls, Owner, Parameters, annotated_type(Inner, _), Type) :-
    !,
    normalized_type(Decls, Owner, Parameters, Inner, Type).
normalized_type(_, Owner, Parameters, Type0, type_ref(Type)) :-
    atom(Type0),
    member(Parameter0, Parameters),
    parameter_parts(Parameter0, Type0, _),
    !,
    parameter_id_for(Owner, Parameters, Type0, ParameterId),
    Type = parameter(ParameterId).
normalized_type(_, _, _, Type, type_ref(primitive(Type))) :-
    atom(Type), scalar_element(Type), !.
normalized_type(Decls, _, _, Type, type_ref(declaration(Id))) :-
    atom(Type), semantic_enum_type_id(Decls, Type, Id), !.
normalized_type(Decls, _, _, Type, type_ref(declaration(Id))) :-
    atom(Type), semantic_decl_id(Decls, relation, Type, Id),
    member(type_decl(Type, _), Decls), !.
normalized_type(_, _, _, Type, type_ref(named(Type))) :-
    anonymous_type_term(Type),
    !.
normalized_type(Decls, _, _, Type, type_ref(application(Id))) :-
    compound(Type), Type =.. [Name | Args],
    semantic_type_constructor_id(Decls, Name, Constructor),
    semantic_application_id(Decls, Constructor, Args, Id), !.
normalized_type(_, _, _, Type, type_ref(named(Type))).

semantic_enum_type_id(Decls, Name, Id) :-
    member(enum_decl(Name, _), Decls),
    !,
    semantic_decl_id(Decls, enum, Name, Id).
semantic_enum_type_id(Decls, Name, Id) :-
    member(semantic_type_rows(Rows), Decls),
    member(declaration(Id, root, Name, enum, _), Rows).

semantic_type_constructor_id(Decls, Name, Id) :-
    generic_enum_constructor_name(Decls, Name),
    !,
    semantic_decl_id(Decls, enum, Name, Id).
semantic_type_constructor_id(Decls, Name, Id) :-
    ( generic_relation_constructor_name(Decls, Name)
    ; builtin_type_constructor(Name)
    ),
    semantic_decl_id(Decls, relation, Name, Id).
semantic_type_constructor_id(Decls, Name, Id) :-
    compiler_derived_constructor(Decls, Name, _),
    semantic_decl_id(Decls, relation, Name, Id).
semantic_type_constructor_id(Decls, Name, Id) :-
    member(semantic_type_rows(Rows), Decls),
    member(declaration(Id, _, Name, relation, compile_time), Rows).

generic_relation_constructor_name(Decls, Name) :-
    member(rel_template(Segments, _, _), Decls),
    atomic_list_concat(Segments, '__', Name).

generic_enum_constructor_name(Decls, Name) :-
    member(rel_template_enum(Segments, _, _), Decls),
    atomic_list_concat(Segments, '__', Name).

type_row_templates(Decls, Rows, Templates) :-
    findall(template(Name, Parameters, Specs),
            ( member(rel_template(Segments, _, Specs), Decls),
              atomic_list_concat(Segments, '__', Name),
              generic_template_parameters(Decls, Rows, Name, Parameters) ),
            Templates).

generic_template_parameters(Decls, Rows, Name, Parameters) :-
    semantic_decl_id(Decls, relation, Name, Owner),
    findall(Ordinal-Parameter,
            ( member(parameter(ParameterId, Owner, Ordinal, ParameterName), Rows),
              findall(Constraint,
                      ( member(Row, Rows),
                        constraint_surface_row(Row, ParameterId, Constraint) ),
                      Constraints),
              Parameter = type_parameter(ParameterName, Constraints) ),
            Ordered),
    keysort(Ordered, Pairs),
    pairs_values(Pairs, Parameters).

constraint_surface_row(constraint(_, ParameterId, InterfaceId), ParameterId,
                       InterfaceName) :-
    id_kind_name(InterfaceId, interface, InterfaceName).
constraint_surface_row(constraint(_, ParameterId, InterfaceId, Patterns),
                       ParameterId, Application) :-
    id_kind_name(InterfaceId, interface, InterfaceName),
    Application =.. [InterfaceName | Patterns].

validate_type_rows(Rows) :-
    forall(member_constraint_row(Rows, _, InterfaceId, _),
           ( memberchk(declaration(InterfaceId, _, _, interface, _), Rows) -> true
           ; id_kind_name(InterfaceId, interface, InterfaceName),
             throw(unsupported_construct(interface_unknown(InterfaceName))) )).
