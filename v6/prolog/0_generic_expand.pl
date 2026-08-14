% Generic expansion closes schema templates before enum expansion.
%
% The artifact table uses typed records.  `lower_artifacts/2` is the one
% boundary where a template's schema records become the program's Decl terms.
% Round one emits declarations only.  Rules remain author-written.
:- module(generic_expand,
          [ expand_generic_in_context/3,
            expand_generic_program/2,
            expand_generic_program_raw/2,
            canonical_type_name/2,
            canonical_type_encoding/2,
            generic_artifact_order/3,
            generated_generic_name/1,
            generic_type_ir/2
          ]).

:- use_module(library(apply)).
:- use_module(library(crypto)).
:- use_module(library(lists)).
:- use_module('0_option_expand', [expand_option_decls/2, scalar_element/1]).
:- use_module('0_type_plane', [unwrapped_column_type/2]).
:- use_module('0_type_ids',
              [ decl_id/3, param_id/4, member_id/4, constraint_id/3,
                impl_id/3, app_id/3, arg_id/3, id_kind_name/3 ]).

expand_generic_in_context(_Context, Program, Expanded) :-
    expand_generic_program(Program, Expanded).

expand_generic_program(prog(Decls0, Rules), prog(Decls, Rules)) :-
    expand_user_templates(Decls0, Rules, _UserInstances, UserDecls),
    generic_fixpoint(UserDecls, Instances, WithMintedDecls),
    validate_generated_name_collisions(UserDecls, Rules, Instances),
    replace_generic_types(WithMintedDecls, Instances, RewrittenDecls),
    generic_artifact_order(Instances, RewrittenDecls, CanonicalDecls),
    merge_flavor_type_rows(Instances, CanonicalDecls, FlavorRowedDecls),
    expand_option_decls(FlavorRowedDecls, OptionDecls),
    retarget_type_decl_mirrors(OptionDecls, Decls).

% Executable comparison arm, written as a second path so the template and
% replacement logic cannot drift apart from the wired entry above.
expand_generic_program_raw(prog(Decls0, Rules), prog(Decls, Rules)) :-
    expand_user_templates(Decls0, Rules, _UserInstances, UserDecls),
    generic_fixpoint(UserDecls, Instances, WithMintedDecls),
    validate_generated_name_collisions(UserDecls, Rules, Instances),
    replace_generic_types(WithMintedDecls, Instances, RewrittenDecls),
    generic_artifact_order(Instances, RewrittenDecls, CanonicalDecls),
    merge_flavor_type_rows(Instances, CanonicalDecls, FlavorRowedDecls),
    expand_option_decls(FlavorRowedDecls, OptionDecls),
    retarget_type_decl_mirrors(OptionDecls, Decls).

% Each ground application of a compile-time rel template mints one ordinary
% relation schema; downstream storage machinery sees no generic construct.
expand_user_templates(Decls0, Rules, Instances, Decls) :-
    generic_type_ir(Decls0, TypeIr),
    validate_type_rows(TypeIr),
    validate_interface_applications(Decls0),
    type_row_templates(Decls0, TypeIr, Templates),
    check_template_application_arities(Decls0, Templates),
    user_template_fixpoint(Decls0, Templates, [], WithInstances, Instances),
    validate_user_template_collisions(Decls0, Rules, Instances),
    judge_template_bounds(WithInstances, Templates, Instances, JudgmentRows),
    maplist(rewrite_user_template_decl(Instances), WithInstances, Rewritten),
    exclude(is_rel_template, Rewritten, RuntimeDecls),
    generic_catalog_decls(TypeIr, Instances, JudgmentRows, CatalogDecls),
    append(RuntimeDecls, CatalogDecls, Decls).

is_rel_template(rel_template(_, _, _)).

% Source terms are accepted here but generic_rel/interface/implementation terms
% are never constructed; ids derive from labels and ordinals, not source order.
generic_type_ir(Decls, Rows) :-
    normalized_type_rows(Decls, Rows).

normalized_type_rows(Decls, Rows) :-
    findall(Row, normalized_declaration_row(Decls, Row), DeclarationRows),
    findall(Row, normalized_parameter_row(Decls, Row), ParameterRows),
    findall(Row, normalized_member_row(Decls, Row), MemberRows),
    findall(Row, normalized_constraint_row(Decls, Row), ConstraintRows),
    normalized_application_rows(Decls, ApplicationRows),
    findall(Row, normalized_implementation_row(Decls, Row), ImplementationRows),
    normalized_derivation_rows(Decls, ApplicationRows, DerivationRows),
    append([DeclarationRows, ParameterRows, MemberRows, ConstraintRows,
            ApplicationRows, ImplementationRows, DerivationRows], Unsorted),
    sort(Unsorted, Rows).

normalized_declaration_row(Decls, declaration(Id, root, Name, relation, materialized)) :-
    member(type_decl(Name, _), Decls),
    decl_id(relation, Name, Id).
normalized_declaration_row(Decls, declaration(Id, root, Name, relation, compile_time)) :-
    member(rel_template(Segments, _, _), Decls),
    atomic_list_concat(Segments, '__', Name),
    decl_id(relation, Name, Id).
normalized_declaration_row(Decls, declaration(Id, root, Name, interface, compile_time)) :-
    member(interface_decl(Name, _), Decls),
    decl_id(interface, Name, Id).
normalized_declaration_row(Decls, declaration(Id, root, ConcreteName, relation, materialized)) :-
    source_generic_application(Decls, Application),
    canonical_type_name(Application, ConcreteName),
    decl_id(relation, ConcreteName, Id).

normalized_parameter_row(Decls, parameter(Id, Owner, Ordinal, Name)) :-
    generic_owner_parameters(Decls, OwnerName, Parameters),
    decl_id(relation, OwnerName, Owner),
    nth1(Ordinal, Parameters, Parameter0),
    parameter_parts(Parameter0, Name, _),
    param_id(Owner, Ordinal, Name, Id).
normalized_parameter_row(Decls, parameter(Id, Owner, Ordinal, Name)) :-
    member(interface_decl(OwnerName, Parameters), Decls),
    decl_id(interface, OwnerName, Owner),
    nth1(Ordinal, Parameters, Parameter0),
    parameter_parts(Parameter0, Name, _),
    param_id(Owner, Ordinal, Name, Id).

normalized_member_row(Decls, member(Id, Owner, Ordinal, Name, Type)) :-
    member(rel_template(Segments, Parameters, Specs), Decls),
    atomic_list_concat(Segments, '__', OwnerName),
    decl_id(relation, OwnerName, Owner),
    nth1(Ordinal, Specs, column(Name, Type0)),
    normalized_type(Decls, Owner, Parameters, Type0, Type),
    member_id(Owner, Ordinal, Name, Id),
    !.
normalized_member_row(Decls, member(Id, Owner, Ordinal, Name, Type)) :-
    member(type_decl(OwnerName, Specs), Decls),
    decl_id(relation, OwnerName, Owner),
    nth1(Ordinal, Specs, col(Name, Type0)),
    normalized_type(Decls, Owner, [], Type0, Type),
    member_id(Owner, Ordinal, Name, Id).

normalized_constraint_row(Decls, constraint(Id, ParameterId, InterfaceId)) :-
    generic_owner_parameters(Decls, OwnerName, Parameters),
    decl_id(relation, OwnerName, Owner),
    nth1(Ordinal, Parameters, Parameter0),
    parameter_parts(Parameter0, Name, Constraints),
    param_id(Owner, Ordinal, Name, ParameterId),
    member(Constraint, Constraints),
    interface_application_name(Constraint, InterfaceName),
    decl_id(interface, InterfaceName, InterfaceId),
    constraint_id(ParameterId, InterfaceId, Id).

normalized_constraint_row(Decls, constraint(Id, ParameterId, InterfaceId)) :-
    member(interface_decl(OwnerName, Parameters), Decls),
    decl_id(interface, OwnerName, Owner),
    nth1(Ordinal, Parameters, Parameter0),
    parameter_parts(Parameter0, Name, Constraints),
    param_id(Owner, Ordinal, Name, ParameterId),
    member(Constraint, Constraints),
    interface_application_name(Constraint, InterfaceName),
    decl_id(interface, InterfaceName, InterfaceId),
    constraint_id(ParameterId, InterfaceId, Id).

normalized_implementation_row(Decls,
                              implementation(Id, Subject, interface_application(InterfaceId))) :-
    member(rel_is_implementation(Ref, Applications), Decls),
    ref_name(Ref, SubjectName),
    decl_id(relation, SubjectName, Subject),
    member(Application, Applications),
    interface_application_name(Application, InterfaceName),
    decl_id(interface, InterfaceName, InterfaceId),
    impl_id(Subject, InterfaceId, Id).

normalized_application_rows(Decls, Rows) :-
    findall(Application, source_generic_application(Decls, Application), Found),
    sort(Found, Applications),
    maplist(normalized_application_row, Applications, Nested),
    append(Nested, Rows).

source_generic_application(Decls, Application) :-
    member(rel_template(_, _, _), Decls),
    member(type_decl(_, Specs), Decls),
    member(col(_, Type), Specs),
    sub_term(Application, Type),
    generic_application_name(Decls, Application).
source_generic_application(Decls, Application) :-
    member(col_type(_, _, Type), Decls),
    sub_term(Application, Type),
    generic_application_name(Decls, Application).

generic_application_name(Decls, Application) :-
    compound(Application),
    functor(Application, Name, Arity),
    member(rel_template(Segments, Parameters, _), Decls),
    atomic_list_concat(Segments, '__', Name),
    length(Parameters, Arity).

normalized_application_row(Application, Rows) :-
    Application =.. [Name | Arguments],
    decl_id(relation, Name, Constructor),
    app_id(Constructor, Arguments, Id),
    ApplicationRow = application(Id, Constructor),
    findall(Row,
            ( nth1(Ordinal, Arguments, Type0),
              normalized_argument_type(Type0, Type),
              arg_id(Id, Ordinal, ArgumentId),
              Row = argument(ArgumentId, Id, Ordinal, Type) ),
            ArgumentRows),
    Rows = [ApplicationRow | ArgumentRows].

normalized_argument_type(Type, type_atom(Type)) :- atom(Type), !.
normalized_argument_type(Type, type_application(Id)) :-
    compound(Type),
    Type =.. [Constructor | Arguments],
    decl_id(relation, Constructor, ConstructorId),
    app_id(ConstructorId, Arguments, Id).

normalized_derivation_rows(Decls, ApplicationRows, Rows) :-
    findall(derived_from(ConcreteId, ApplicationId),
            ( member(application(ApplicationId, ConstructorId), ApplicationRows),
              id_kind_name(ConstructorId, relation, ConstructorName),
              source_application_by_constructor(Decls, ConstructorName, Application),
              Application =.. [_ | Arguments],
              app_id(ConstructorId, Arguments, ApplicationId),
              canonical_type_name(Application, ConcreteName),
              decl_id(relation, ConcreteName, ConcreteId) ),
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
    atom(Type), decl_id(relation, Type, Id),
    member(type_decl(Type, _), Decls), !.
normalized_type(_, _, _, Type, type_ref(application(Id))) :-
    compound(Type), Type =.. [Name | Args],
    decl_id(relation, Name, Constructor),
    app_id(Constructor, Args, Id), !.
normalized_type(_, _, _, Type, type_ref(named(Type))).

type_row_templates(Decls, Rows, Templates) :-
    findall(template(Name, Parameters, Specs),
            ( member(rel_template(Segments, _, Specs), Decls),
              atomic_list_concat(Segments, '__', Name),
              generic_template_parameters(Rows, Name, Parameters) ),
            Templates).

generic_template_parameters(Rows, Name, Parameters) :-
    decl_id(relation, Name, Owner),
    findall(Ordinal-Parameter,
            ( member(parameter(ParameterId, Owner, Ordinal, ParameterName), Rows),
              findall(ConstraintName,
                      ( member(constraint(_, ParameterId, ConstraintId), Rows),
                        id_kind_name(ConstraintId, interface, ConstraintName) ),
                      Constraints),
              Parameter = type_parameter(ParameterName, Constraints) ),
            Ordered),
    keysort(Ordered, Pairs),
    pairs_values(Pairs, Parameters).

validate_type_rows(Rows) :-
    findall(Name-Arity,
            ( member(declaration(Id, _, Name, interface, _), Rows),
              findall(_, member(parameter(_, Id, _, _), Rows), Parameters),
              length(Parameters, Arity) ), Interfaces),
    validate_unique_interface_names(Interfaces),
    forall(member(constraint(_, _, InterfaceId), Rows),
           ( memberchk(declaration(InterfaceId, _, _, interface, _), Rows) -> true
           ; id_kind_name(InterfaceId, interface, InterfaceName),
             throw(unsupported_construct(interface_unknown(InterfaceName))) )),
    forall(member(implementation(_, _, interface_application(InterfaceId)), Rows),
           ( memberchk(declaration(InterfaceId, _, _, interface, _), Rows) -> true
           ; id_kind_name(InterfaceId, interface, InterfaceName),
             throw(unsupported_construct(interface_unknown(InterfaceName))) )),
    validate_unique_implementations(Rows).

validate_unique_interface_names(Interfaces) :-
    ( select(Name-_, Interfaces, Rest), memberchk(Name-_, Rest)
    -> throw(unsupported_construct(interface_duplicate(Name)))
    ; true ).

validate_unique_implementations(TypeIr) :-
    findall(Subject-Interface,
            member(implementation(_, Subject, interface_application(Interface)), TypeIr),
            Implementations),
    ( select(Item, Implementations, Rest), memberchk(Item, Rest)
    -> throw(unsupported_construct(interface_implementation_duplicate(Item)))
    ; true ).

% Checked at the source terms: the normalized implementation row drops the
% application's explicit arguments, so arity is invisible in the row graph.
validate_interface_applications(Decls) :-
    findall(Name-Arity,
            ( member(interface_decl(Name, Parameters), Decls),
              length(Parameters, Arity) ),
            Interfaces),
    forall(( member(rel_is_implementation(_, Applications), Decls),
             member(Application, Applications) ),
           validate_interface_application(Application, Interfaces)).

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

ref_name(Name/_, Name) :- !.
ref_name(Name, Name).

% Instances minted after the source scan carry their own rows: the graph is the
% only generic freight leaving expansion.
generic_catalog_decls(TypeIr, Instances, JudgmentRows, Decls) :-
    instance_type_rows(Instances, InstanceRows),
    append([TypeIr, InstanceRows, JudgmentRows], Unsorted),
    sort(Unsorted, Rows),
    ( Rows == [] -> Decls = [] ; Decls = [semantic_type_rows(Rows)] ).

instance_type_rows(Instances, Rows) :-
    findall(Row,
            ( member(Application, Instances), instance_type_row(Application, Row) ),
            Rows).

instance_type_row(Application, Row) :-
    normalized_application_row(Application, ApplicationRows),
    member(Row, ApplicationRows).
instance_type_row(Application, declaration(Id, root, Name, relation, materialized)) :-
    canonical_type_name(Application, Name),
    decl_id(relation, Name, Id).
instance_type_row(Application, derived_from(ConcreteId, ApplicationId)) :-
    Application =.. [ConstructorName | Arguments],
    decl_id(relation, ConstructorName, Constructor),
    app_id(Constructor, Arguments, ApplicationId),
    canonical_type_name(Application, ConcreteName),
    decl_id(relation, ConcreteName, ConcreteId).

% NO compile_time row for the builtin constructor: semantic_generic_instance
% in lower.pl requires one, so its absence keeps list mints out of the catalog.
merge_flavor_type_rows(Instances, Decls0, Decls) :-
    flavor_type_rows(Instances, Rows),
    (   Rows == []
    ->  Decls = Decls0
    ;   memberchk(semantic_type_rows(_), Decls0)
    ->  maplist(merge_one_flavor_type_rows(Rows), Decls0, Decls)
    ;   append(Decls0, [semantic_type_rows(Rows)], Decls)
    ).

merge_one_flavor_type_rows(FlavorRows, semantic_type_rows(Rows0),
                           semantic_type_rows(Rows)) :-
    !,
    append(Rows0, FlavorRows, Unsorted),
    sort(Unsorted, Rows).
merge_one_flavor_type_rows(_, Decl, Decl).

flavor_type_rows(Instances, Rows) :-
    instance_type_rows(Instances, ApplicationRows),
    findall(Row, flavor_mint_row(Instances, Row), MintRows),
    append(ApplicationRows, MintRows, Unsorted),
    sort(Unsorted, Rows).

flavor_mint_row(Instances, Row) :-
    member(Type, Instances),
    Type =.. [ConstructorName | Arguments],
    decl_id(relation, ConstructorName, Constructor),
    app_id(Constructor, Arguments, ApplicationId),
    list_flavor_suffix(Type, Suffix),
    Suffix \== list,
    flavor_ref(Type, Suffix, MintName/_),
    decl_id(relation, MintName, MintId),
    member(Row,
           [ declaration(MintId, root, MintName, relation, materialized),
             derived_from(MintId, ApplicationId) ]).

user_template_fixpoint(Decls, Templates, Seen, AllDecls, Instances) :-
    user_template_instances(Decls, Templates, Found),
    subtract(Found, Seen, New),
    maplist(instantiate_user_template(Templates, Decls), New, DeclLists),
    append(DeclLists, NewDecls),
    append(Seen, New, Seen1),
    append(Decls, NewDecls, Decls1),
    ( New == []
    -> AllDecls = Decls,
       Instances = Seen
    ; user_template_fixpoint(Decls1, Templates, Seen1, AllDecls, Instances)
    ).

user_template_instances(Decls, Templates, Instances) :-
    findall(Application,
            ( member(col_type(_, _, Type), Decls),
              sub_term(Application, Type),
              compound(Application),
              functor(Application, Name, Arity),
              member(template(Name, Parameters, _), Templates),
              length(Parameters, Arity),
              check_ground_generic(Application) ),
            Found),
    sort(Found, Instances).

check_template_application_arities(Decls, Templates) :-
    ( member(col_type(_, _, Type), Decls),
      sub_term(Application, Type),
      compound(Application),
      functor(Application, Name, Actual),
      member(template(Name, Parameters, _), Templates),
      length(Parameters, Expected),
      Actual =\= Expected
    -> throw(unsupported_construct(
                 generic_template_arity(Name, Expected, Actual)))
    ; true
    ).

instantiate_user_template(Templates, _SourceDecls, Application, Decls) :-
    Application =.. [Name | Arguments],
    memberchk(template(Name, Parameters, Specs), Templates),
    maplist(template_parameter_name, Parameters, ParameterNames),
    pairs_keys_values(Bindings, ParameterNames, Arguments),
    canonical_type_name(Application, ConcreteName),
    maplist(substitute_template_column(Bindings), Specs, Columns),
    maplist(column_mirror, Columns, Mirror),
    length(Columns, Arity),
    maplist(column_decl(ConcreteName/Arity), Columns, ColumnDecls),
    Decls = [type_decl(ConcreteName, Mirror) | ColumnDecls].

substitute_template_column(Bindings, column(Name, Type0), column(Name, Type)) :-
    substitute_template_type(Bindings, Type0, Type).

substitute_template_type(Bindings, Type0, Type) :-
    atom(Type0),
    memberchk(Type0-Type, Bindings),
    !.
substitute_template_type(_, Type, Type) :- atom(Type), !.
substitute_template_type(Bindings, Type0, Type) :-
    Type0 =.. [Constructor | Args0],
    maplist(substitute_template_type(Bindings), Args0, Args),
    Type =.. [Constructor | Args].

column_mirror(column(Name, Type), col(Name, Type)).
column_decl(Ref, column(Name, Type), col_type(Ref, Name, Type)).

template_parameter_name(type_parameter(Name, _), Name) :- !.
template_parameter_name(Name, Name).

parameter_constraints(type_parameter(_, Constraints), Constraints) :- !.
parameter_constraints(_, []).

% Bounds are judged AFTER the fixpoint on the completed declarations, so a
% minted inner instance can discharge an outer application's bound.
judge_template_bounds(Decls, Templates, Instances, Rows) :-
    foldl(judge_application_bounds(Decls, Templates), Instances, [], Rows0),
    sort(Rows0, Rows).

judge_application_bounds(Decls, Templates, Application, Rows0, Rows) :-
    Application =.. [Name | Arguments],
    memberchk(template(Name, Parameters, _), Templates),
    decl_id(relation, Name, Constructor),
    app_id(Constructor, Arguments, AppId),
    findall(Judgment,
            obligation_judgment(Decls, Constructor, AppId, Parameters,
                                Arguments, Judgment),
            Judged),
    (   member(unresolved(Ordinal, ArgType, Interface), Judged)
    ->  throw(unsupported_construct(
                  generic_bound_unsatisfied(ArgType, Interface,
                      path([template(Name), application(Application),
                            argument(Ordinal, ArgType)]))))
    ;   true
    ),
    findall(Row, substitution_row(Constructor, AppId, Parameters, Arguments,
                                  Row), SubstitutionRows),
    findall(Row,
            ( member(judged(_, Obligation, Resolution), Judged),
              ( Row = Obligation ; Row = Resolution ) ),
            ObligationRows),
    append([[well_formed(AppId)], SubstitutionRows, ObligationRows, Rows0],
           Rows).

substitution_row(Constructor, AppId, Parameters, Arguments,
                 substitution(AppId, ParameterId, ArgType)) :-
    nth1(Ordinal, Parameters, Parameter),
    parameter_parts(Parameter, ParameterName, _),
    param_id(Constructor, Ordinal, ParameterName, ParameterId),
    nth1(Ordinal, Arguments, ArgType).

obligation_judgment(Decls, _Constructor, AppId, Parameters, Arguments,
                    Judgment) :-
    nth1(Ordinal, Parameters, Parameter),
    parameter_constraints(Parameter, Constraints),
    nth1(Ordinal, Arguments, ArgType),
    member(Constraint, Constraints),
    interface_application_name(Constraint, Interface),
    decl_id(interface, Interface, InterfaceId),
    arg_id(AppId, Ordinal, ArgId),
    constraint_id(ArgId, InterfaceId, ObligationId),
    (   bound_evidence(Decls, ArgType, Interface, Evidence)
    ->  Judgment = judged(Ordinal,
                          obligation(ObligationId, AppId, InterfaceId,
                                     ArgType),
                          resolved_by(ObligationId, Evidence))
    ;   Judgment = unresolved(Ordinal, ArgType, Interface)
    ).

bound_evidence(Decls, ArgType, Interface, impl(ImplId)) :-
    atom(ArgType),
    member(rel_is_implementation(Ref, Applications), Decls),
    ref_name(Ref, ArgType),
    member(Application, Applications),
    interface_application_name(Application, Interface),
    !,
    decl_id(relation, ArgType, Subject),
    decl_id(interface, Interface, InterfaceId),
    impl_id(Subject, InterfaceId, ImplId).
bound_evidence(Decls, ArgType, json_encodable, structural(ArgType)) :-
    json_encodable_type(Decls, ArgType, []).

json_encodable_type(_, Type, _) :-
    atom(Type), scalar_element(Type), !.
json_encodable_type(Decls, option(Type), Seen) :- !,
    json_encodable_type(Decls, Type, Seen).
json_encodable_type(Decls, json_list(Type), Seen) :- !,
    json_encodable_type(Decls, Type, Seen).
json_encodable_type(Decls, Name, _) :-
    atom(Name),
    member(rel_is_implementation(Name/_, Applications), Decls),
    memberchk(json_encodable, Applications),
    !.
json_encodable_type(_, Name, Seen) :- memberchk(Name, Seen), !.
json_encodable_type(Decls, Name, Seen) :-
    atom(Name),
    named_type_columns(Decls, Name, ColumnTypes),
    maplist(json_encodable_type_with_seen(Decls, [Name | Seen]), ColumnTypes),
    !.
json_encodable_type(Decls, Name, Seen) :-
    atom(Name),
    member(enum_decl(Name, Variants), Decls),
    enum_payload_types(Variants, PayloadTypes),
    maplist(json_encodable_type_with_seen(Decls, [Name | Seen]), PayloadTypes).
% A user-template application closes over its MINTED instance's columns; the
% guard keeps builtin list flavors refused as before.
json_encodable_type(Decls, Type, Seen) :-
    compound(Type),
    generic_application_name(Decls, Type),
    \+ memberchk(Type, Seen),
    canonical_type_name(Type, Name),
    json_encodable_type(Decls, Name, [Type | Seen]).

json_encodable_type_with_seen(Decls, Seen, Type) :-
    json_encodable_type(Decls, Type, Seen).

named_type_columns(Decls, Name, ColumnTypes) :-
    ( member(type_decl(Name, Specs), Decls)
    -> findall(Type, member(col(_, Type), Specs), ColumnTypes)
    ; findall(Type, member(col_type(Name/_, _, Type), Decls), ColumnTypes),
      ColumnTypes \== [] ).

enum_payload_types(Variants, Types) :-
    findall(Type,
            ( enum_variant_term(Variants, Variant),
              Variant =.. [_ | Fields],
              member(_:Type, Fields) ),
            Types).

enum_variant_term((Left ; Right), Variant) :- !,
    ( enum_variant_term(Left, Variant) ; enum_variant_term(Right, Variant) ).
enum_variant_term(Variant, Variant).

rewrite_user_template_decl(Instances,
                           col_type(Ref, Column, Type0),
                           col_type(Ref, Column, Type)) :-
    !,
    rewrite_user_template_type(Instances, Type0, Type).
rewrite_user_template_decl(_, Decl, Decl).

rewrite_user_template_type(Instances, Type0, Type) :-
    memberchk(Type0, Instances),
    !,
    canonical_type_name(Type0, Type).
rewrite_user_template_type(_, Type, Type) :- atom(Type), !.
rewrite_user_template_type(Instances, Type0, Type) :-
    Type0 =.. [Constructor | Args0],
    maplist(rewrite_user_template_type(Instances), Args0, Args),
    Type =.. [Constructor | Args].

validate_user_template_collisions(Decls, Rules, Instances) :-
    maplist(canonical_type_name, Instances, Names),
    throw_on_author_collision(Names, Decls, Rules).

throw_on_author_collision(Names, Decls, Rules) :-
    findall(Name, author_decl_or_rule_name(Decls, Rules, Name), AuthorNames),
    ( member(Name, Names), memberchk(Name, AuthorNames)
    -> throw(unsupported_construct(generic_generated_name_collision(Name)))
    ; true
    ).

% A worklist is represented by the canonical sorted instance list.  The four
% list constructors are term-door-only lab constructors.  No parser spelling
% is claimed here.

% Discovery fixes over minted decls (an outer member's value column is itself
% a list), so each pass mints the not-yet-lowered instances and re-scans.
generic_fixpoint(SourceDecls, Instances, AllDecls) :-
    check_interned_set_rel_elements(SourceDecls),
    generic_fixpoint_(SourceDecls, [], AllDecls, Instances).

% A rel element does not belong inside the interned-set value dictionary: the
% rel row already interns it (the rel id IS the interned id). Named here rather
% than forced through the redundant content-addressed dictionary.
check_interned_set_rel_elements(SourceDecls) :-
    declared_type_names(SourceDecls, Names),
    (   member(col_type(_, _, Type), SourceDecls),
        unwrapped_column_type(Type, list_interned_set(Element)),
        memberchk(Element, Names)
    ->  throw(unsupported_construct(list_interned_set_relation_element(Element)))
    ;   true
    ).

declared_type_names(Decls, Names) :-
    findall(Name, member(type_decl(Name, _), Decls), Names).

generic_fixpoint_(Decls, MintedSoFar, AllDecls, Instances) :-
    generic_type_instances(Decls, AllFound),
    subtract(AllFound, MintedSoFar, NewInstances),
    maplist(template_artifacts, NewInstances, ArtifactLists),
    append(ArtifactLists, Artifacts),
    lower_artifacts(Artifacts, NewDecls),
    append(MintedSoFar, NewInstances, AllMintedInstances),
    append(Decls, NewDecls, NextDecls),
    ( NewInstances == []
    -> AllDecls = Decls,
       Instances = AllMintedInstances
    ; generic_fixpoint_(NextDecls, AllMintedInstances, AllDecls, Instances)
    ).

generic_type_instances(Decls, Instances) :-
    findall(Type, ( member(col_type(_, _, Type), Decls), generic_type(Type) ),
            Found),
    maplist(check_ground_generic, Found),
    findall(Instance,
            ( member(Type, Found), generic_dependency(Type, Instance) ),
            FoundInstances),
    sort(FoundInstances, Instances).

generic_type(list(_)).
generic_type(list_entity_dense_sequence(_)).
generic_type(list_interned_set(_)).
generic_type(list_entity_linked_sequence(_)).
generic_type(option(Type)) :- contains_list_flavor(Type).

contains_list_flavor(Type) :-
    once(( unwrapped_column_type(Type, Inner), list_flavor(Inner) )).

generic_dependency(option(Type), Instance) :-
    generic_dependency(Type, Instance).
generic_dependency(list(Element), list(Element)).
generic_dependency(Type, Type) :- named_list_flavor(Type).

list_flavor(list(_)).
list_flavor(Type) :- named_list_flavor(Type).

named_list_flavor(list_entity_dense_sequence(_)).
named_list_flavor(list_interned_set(_)).
named_list_flavor(list_entity_linked_sequence(_)).

check_ground_generic(Type) :-
    ( ground(Type) -> true
    ; throw(unsupported_construct(generic_type_not_ground(Type)))
    ).

% Typed artifact vocabulary.  Additional templates add records here and the
% lowering relation remains the sole place coupled to Decl syntax.
template_artifacts(Type, Artifacts) :-
    list_flavor(Type),
    !,
    list_flavor_artifacts(Type, Artifacts).
template_artifacts(option(_), []).

list_flavor_artifacts(list(Element), Artifacts) :-
    flavor_ref(list(Element), list, Entity),
    flavor_ref(list(Element), member, Member),
    Artifacts = [ artifact(decl(col_type(Entity, id, int))),
                  artifact(decl(keyed(Entity, [1]))),
                  artifact(decl(col_type(Member, list_id, int))),
                  artifact(decl(col_type(Member, idx, int))),
                  artifact(decl(col_type(Member, value, Element))),
                  artifact(decl(keyed(Member, [1, 2]))) ].
list_flavor_artifacts(list_entity_dense_sequence(Element), Artifacts) :-
    flavor_ref(list_entity_dense_sequence(Element), list, Entity),
    flavor_ref(list_entity_dense_sequence(Element), member, Member),
    flavor_ref(list_entity_dense_sequence(Element), owner, Owner),
    flavor_ref(list_entity_dense_sequence(Element), refcount, Refcount),
    Artifacts = [ artifact(decl(col_type(Entity, id, int))),
                  artifact(decl(keyed(Entity, [1]))),
                  artifact(decl(col_type(Member, list_id, int))),
                  artifact(decl(col_type(Member, idx, int))),
                  artifact(decl(col_type(Member, value, Element))),
                  artifact(decl(keyed(Member, [1, 2]))),
                  artifact(decl(col_type(Owner, owner_id, int))),
                  artifact(decl(col_type(Owner, list_id, int))),
                  artifact(decl(keyed(Owner, [1, 2]))),
                  artifact(decl(col_type(Refcount, list_id, int))),
                  artifact(decl(col_type(Refcount, count, int))),
                  artifact(decl(keyed(Refcount, [1]))) ].
list_flavor_artifacts(list_interned_set(Element), Artifacts) :-
    flavor_ref(list_interned_set(Element), list, Entity),
    flavor_ref(list_interned_set(Element), value, Value),
    flavor_ref(list_interned_set(Element), member, Member),
    Artifacts = [ artifact(decl(col_type(Entity, content_id, int))),
                  artifact(decl(keyed(Entity, [1]))),
                  artifact(decl(col_type(Value, id, int))),
                  artifact(decl(col_type(Value, value, Element))),
                  artifact(decl(keyed(Value, [2]))),
                  artifact(decl(col_type(Member, content_id, int))),
                  artifact(decl(col_type(Member, value_id, int))),
                  artifact(decl(keyed(Member, [1, 2]))) ].
list_flavor_artifacts(list_entity_linked_sequence(Element), Artifacts) :-
    flavor_ref(list_entity_linked_sequence(Element), list, Entity),
    flavor_ref(list_entity_linked_sequence(Element), member, Member),
    flavor_ref(list_entity_linked_sequence(Element), link, Link),
    Artifacts = [ artifact(decl(col_type(Entity, id, int))),
                  artifact(decl(keyed(Entity, [1]))),
                  artifact(decl(col_type(Member, member_id, int))),
                  artifact(decl(col_type(Member, list_id, int))),
                  artifact(decl(col_type(Member, value, Element))),
                  artifact(decl(keyed(Member, [1]))),
                  artifact(decl(col_type(Link, before_member_id, int))),
                  artifact(decl(col_type(Link, after_member_id, int))),
                  artifact(decl(keyed(Link, [1, 2]))) ].

flavor_ref(Type, list, Name/1) :- canonical_type_name(Type, Name).
flavor_ref(Type, Suffix, Name/Arity) :-
    Suffix \== list,
    canonical_type_name(Type, Base),
    atomic_list_concat([Base, Suffix], '__', Name),
    flavor_ref_arity(Type, Suffix, Arity).

% Arity must equal the column count list_flavor_artifacts/2 declares for the
% same suffix; the interned-set member row is (content_id, value_id).
flavor_ref_arity(list_interned_set(_), member, 2) :- !.
flavor_ref_arity(_, member, 3).
flavor_ref_arity(_, owner, 2).
flavor_ref_arity(_, refcount, 2).
flavor_ref_arity(_, value, 2).
flavor_ref_arity(_, link, 2).

lower_artifacts([], []).
lower_artifacts([artifact(decl(Decl)) | Rest], [Decl | Decls]) :-
    lower_artifacts(Rest, Decls).

% Author declarations retain source order; generated declarations follow them.
generic_artifact_order([], Decls, Decls).
generic_artifact_order([_ | _], Decls, Ordered) :-
    partition(minted_decl, Decls, Minted, Author),
    append(Author, Minted, Ordered).

minted_decl(col_type(Ref/_, _, _)) :- generated_generic_name(Ref).
minted_decl(keyed(Ref/_, _)) :- generated_generic_name(Ref).

validate_generated_name_collisions(Decls, Rules, Instances) :-
    findall(Name,
            ( member(Type, Instances), template_generated_name(Type, Name) ),
            GeneratedNames),
    sort(GeneratedNames, UniqueNames),
    length(GeneratedNames, GeneratedCount),
    length(UniqueNames, GeneratedCount),
    throw_on_author_collision(GeneratedNames, Decls, Rules).

template_generated_name(Type, Name) :-
    list_flavor_suffix(Type, Suffix),
    flavor_ref(Type, Suffix, Name/_).

list_flavor_suffix(list(_), list).
list_flavor_suffix(list(_), member).
list_flavor_suffix(list_entity_dense_sequence(_), list).
list_flavor_suffix(list_entity_dense_sequence(_), member).
list_flavor_suffix(list_entity_dense_sequence(_), owner).
list_flavor_suffix(list_entity_dense_sequence(_), refcount).
list_flavor_suffix(list_interned_set(_), list).
list_flavor_suffix(list_interned_set(_), value).
list_flavor_suffix(list_interned_set(_), member).
list_flavor_suffix(list_entity_linked_sequence(_), list).
list_flavor_suffix(list_entity_linked_sequence(_), member).
list_flavor_suffix(list_entity_linked_sequence(_), link).

author_decl_or_rule_name(Decls, _, Name) :-
    member(Decl, Decls),
    plain_decl_name(Decl, Name).
author_decl_or_rule_name(_, Rules, Name) :-
    member(Rule, Rules),
    Rule =.. [_, Head | _],
    compound(Head), functor(Head, Name, _).

plain_decl_name(col_type(Name/_, _, _), Name).
plain_decl_name(keyed(Name/_, _), Name).
plain_decl_name(kind(Name/_, _), Name).
plain_decl_name(keep(Name/_, _), Name).

replace_generic_types(Decls0, Instances, Decls) :-
    maplist(replace_generic_decl(Instances), Decls0, Decls).

replace_generic_decl(Instances,
                     col_type(Ref, Column, Type0),
                     col_type(Ref, Column, Type)) :-
    !,
    replace_generic_type(Type0, Instances, Type).
replace_generic_decl(_, Decl, Decl).

retarget_type_decl_mirrors(Decls0, Decls) :-
    maplist(retarget_type_decl_mirror(Decls0), Decls0, Decls).

% The mirror states the rel's stored columns, so it is re-read whole from the
% expanded col_type rows: a rename and a drop land by the same read.
retarget_type_decl_mirror(Decls,
                          type_decl(RelName, Specs0),
                          type_decl(RelName, Specs)) :-
    !,
    (   expanded_relation_specs(Decls, RelName, Rebuilt)
    ->  Specs = Rebuilt
    ;   Specs = Specs0
    ).
retarget_type_decl_mirror(_, Decl, Decl).

expanded_relation_specs(Decls, RelName, Specs) :-
    once(member(col_type(RelName/Arity, _, _), Decls)),
    findall(col(Column, Type),
            ( member(col_type(RelName/Arity, Column, Stored), Decls),
              mirror_column_type(Decls, Stored, Type) ),
            Specs),
    length(Specs, Arity).
% A rel reached in column position needs stored columns: identity is key(...)
% or the full row, and a zero-column row has neither.
expanded_relation_specs(Decls, RelName, _) :-
    \+ member(col_type(RelName/_, _, _), Decls),
    memberchk(option_column(RelName/_, _, _), Decls),
    throw(unsupported_construct(
            reference_target_has_no_columns(RelName/0))).

mirror_column_type(Decls, Type, int) :-
    memberchk(enum_decl(Type, _), Decls),
    !.
mirror_column_type(_, Type, Type).

replace_generic_type(option(Type0), Instances, option(Name)) :-
    !,
    replace_option_element(Type0, Instances, Name).
% A bare list column IS its list entity's id, so the column type lowers to the
% integer id exactly like every companion `_id` column the flavors already emit.
replace_generic_type(Type, Instances, int) :-
    list_flavor(Type),
    memberchk(Type, Instances),
    !.
replace_generic_type(Type, _, Type).

% option(list(T)) keeps the entity name so its companion split rel targets the
% minted list; other generic option elements hand back to the general rewrite.
replace_option_element(Type0, Instances, Name) :-
    (   list_flavor(Type0),
        memberchk(Type0, Instances)
    ->  canonical_type_name(Type0, Name)
    ;   replace_generic_type(Type0, Instances, Name)
    ).

% Readable stem plus a 64-bit SHA-256 prefix.  The digest input is the complete
% length-prefixed structural encoding.  `validate_generated_name_collisions/3`
% rejects a truncated-digest collision before lowering any declaration.
canonical_type_name(Type, Name) :-
    canonical_type_encoding(Type, Encoding),
    readable_stem(Type, Stem),
    crypto_data_hash(Encoding, FullDigest, [algorithm(sha256)]),
    sub_atom(FullDigest, 0, 16, _, Digest),
    atomic_list_concat(['__gen_', Stem, Digest], '_', Name).

canonical_type_encoding(Type, Encoding) :-
    type_encoding_codes(Type, Codes),
    atom_codes(Encoding, Codes).

type_encoding_codes(Type, Codes) :-
    atom(Type), !,
    atom_codes(Type, AtomCodes), length(AtomCodes, Length),
    number_codes(Length, LengthCodes),
    append([[0'a], LengthCodes, [0':], AtomCodes], Codes).
type_encoding_codes(Type, Codes) :-
    compound(Type),
    Type =.. [Constructor | Args],
    atom_codes(Constructor, ConstructorCodes), length(ConstructorCodes, Length),
    number_codes(Length, LengthCodes),
    maplist(type_encoding_codes, Args, ArgCodes), append(ArgCodes, FlatArgs),
    length(Args, Arity), number_codes(Arity, ArityCodes),
    append([[0'c], LengthCodes, [0':], ConstructorCodes, [0'/], ArityCodes,
            [91], FlatArgs, [93]], Codes).

readable_stem(Type, Stem) :-
    atom(Type), !, Stem = Type.
readable_stem(Type, Stem) :-
    Type =.. [Constructor | Args],
    maplist(readable_stem, Args, Stems),
    atomic_list_concat([Constructor | Stems], '_', Stem).

generated_generic_name(Name) :-
    atom(Name), sub_atom(Name, 0, _, _, '__gen_').
