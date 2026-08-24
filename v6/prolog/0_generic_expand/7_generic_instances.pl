% Each ground application of a compile-time rel template mints one ordinary
% relation schema; downstream storage machinery sees no generic construct.
expand_user_templates(Decls0, Rules, Instances, Decls) :-
    generic_type_ir(Decls0, TypeIr),
    compile_type_plane(Decls0, TypeIr, ValidationPlane),
    validate_type_rows(TypeIr),
    validate_compile_type_plane(ValidationPlane),
    validate_interface_applications(Decls0),
    type_row_templates(Decls0, TypeIr, Templates),
    check_template_application_arities(Decls0, Templates),
    user_template_fixpoint(Decls0, Templates, [], WithInstances, Instances),
    validate_user_template_collisions(Decls0, Rules, Instances),
    % The proof relation sees the fixpoint declarations, including every
    % concrete generic instance.  It is compiler-local and is never appended
    % to the runtime declaration list.
    compile_type_plane(WithInstances, TypeIr, ProofPlane),
    judge_template_bounds(ProofPlane, Templates, Instances, JudgmentRows),
    maplist(rewrite_user_template_decl(Instances), WithInstances, Rewritten),
    exclude(expansion_only_decl, Rewritten, RuntimeDecls),
    generic_catalog_decls(WithInstances, TypeIr, Instances, JudgmentRows, CatalogDecls),
    append(RuntimeDecls, CatalogDecls, Decls).

is_rel_template(rel_template(_, _, _)).
is_rel_template_enum(rel_template_enum(_, _, _)).
expansion_only_decl(Decl) :- is_rel_template(Decl).

% Instances minted after the source scan carry their own rows: the graph is the
% only generic freight leaving expansion.
generic_catalog_decls(SourceDecls, TypeIr, Instances, JudgmentRows, Decls) :-
    instance_type_rows(SourceDecls, Instances, InstanceRows),
    append([TypeIr, InstanceRows, JudgmentRows], Unsorted),
    sort(Unsorted, Rows),
    ( Rows == [] -> Decls = [] ; Decls = [semantic_type_rows(Rows)] ).

instance_type_rows(Decls, Instances, Rows) :-
    findall(Row,
            ( member(Application, Instances), instance_type_row(Decls, Application, Row) ),
            Rows).

instance_type_row(Decls, Application, Row) :-
    normalized_application_row(Decls, Application, ApplicationRows),
    member(Row, ApplicationRows).
instance_type_row(Decls, Application, declaration(Id, root, Name, relation, materialized)) :-
    canonical_type_name(Application, Name),
    semantic_decl_id(Decls, relation, Name, Id).
instance_type_row(Decls, Application, derived_from(ConcreteId, ApplicationId)) :-
    Application =.. [ConstructorName | Arguments],
    semantic_decl_id(Decls, relation, ConstructorName, Constructor),
    semantic_application_id(Decls, Constructor, Arguments, ApplicationId),
    canonical_type_name(Application, ConcreteName),
    semantic_decl_id(Decls, relation, ConcreteName, ConcreteId).

% NO compile_time row for the builtin constructor: semantic_generic_instance
% in lower.pl requires one, so its absence keeps list mints out of the catalog.
merge_flavor_type_rows(Instances, Decls0, Decls) :-
    flavor_type_rows(Decls0, Instances, Rows),
    (   Rows == []
    ->  Decls = Decls0
    ;   memberchk(semantic_type_rows(_), Decls0)
    ->  maplist(merge_one_flavor_type_rows(Rows), Decls0, Decls)
    ;   append(Decls0, [semantic_type_rows(Rows)], Decls)
    ).

template_parameters(rel_template(_, Parameters, _), Parameters).
template_parameters(rel_template_enum(_, Parameters, _), Parameters).

merge_one_flavor_type_rows(FlavorRows, semantic_type_rows(Rows0),
                           semantic_type_rows(Rows)) :-
    !,
    append(Rows0, FlavorRows, Unsorted),
    sort(Unsorted, Rows).
merge_one_flavor_type_rows(_, Decl, Decl).

flavor_type_rows(Decls, Instances, Rows) :-
    instance_type_rows(Decls, Instances, ApplicationRows),
    findall(Row, flavor_mint_row(Decls, Instances, Row), MintRows),
    append(ApplicationRows, MintRows, Unsorted),
    sort(Unsorted, Rows).

flavor_mint_row(Decls, Instances, Row) :-
    member(Type, Instances),
    Type =.. [ConstructorName | Arguments],
    semantic_decl_id(Decls, relation, ConstructorName, Constructor),
    semantic_application_id(Decls, Constructor, Arguments, ApplicationId),
    list_flavor_suffix(Type, Suffix),
    Suffix \== list,
    flavor_ref(Type, Suffix, MintName/_),
    semantic_decl_id(Decls, relation, MintName, MintId),
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
            ( generic_source_type(Decls, Type),
              sub_term(Application, Type),
              compound(Application),
              functor(Application, Name, Arity),
              member(template(Name, Parameters, _), Templates),
              length(Parameters, Arity),
              check_ground_generic(Application) ),
            Found),
    sort(Found, Instances).

check_template_application_arities(Decls, Templates) :-
    ( generic_source_type(Decls, Type),
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
judge_template_bounds(Plane, Templates, Instances, Rows) :-
    Plane = type_plane(Decls, _),
    foldl(judge_application_bounds(Decls, Plane, Templates), Instances, [], Rows0),
    sort(Rows0, Rows).

judge_application_bounds(Decls, Plane, Templates, Application, Rows0, Rows) :-
    Application =.. [Name | Arguments],
    memberchk(template(Name, Parameters, _), Templates),
    semantic_decl_id(Decls, relation, Name, Constructor),
    semantic_application_id(Decls, Constructor, Arguments, AppId),
    findall(Judgment,
            obligation_judgment(Decls, Plane, Constructor, AppId, Parameters,
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

obligation_judgment(Decls, Plane, _Constructor, AppId, Parameters, Arguments,
                    Judgment) :-
    nth1(Ordinal, Parameters, Parameter),
    parameter_constraints(Parameter, Constraints),
    nth1(Ordinal, Arguments, ArgType),
    member(Constraint, Constraints),
    interface_application_parts(Constraint, Interface, Patterns),
    semantic_decl_id(Decls, interface, Interface, InterfaceId),
    arg_id(AppId, Ordinal, ArgId),
    constraint_obligation_id(Decls, ArgId, InterfaceId, Patterns, ObligationId),
    (   compile_type_query(Plane,
                           conforms(ArgType,
                                    interface_pattern(Interface, Patterns)),
                           Evidence)
    ->  obligation_judgment_row(Patterns, Ordinal, ObligationId, AppId,
                                InterfaceId, ArgType, Evidence, Judgment)
    ;   Judgment = unresolved(Ordinal, ArgType, Constraint)
    ).

constraint_obligation_id(_, ArgId, InterfaceId, [], Id) :-
    constraint_id(ArgId, InterfaceId, Id).
constraint_obligation_id(Decls, ArgId, InterfaceId, Patterns, Id) :-
    Patterns \== [],
    semantic_application_id(Decls, InterfaceId, Patterns, InterfaceApplicationId),
    constraint_id(ArgId, InterfaceApplicationId, Id).

obligation_judgment_row([], Ordinal, ObligationId, AppId, InterfaceId,
                        ArgType, Evidence,
                        judged(Ordinal,
                               obligation(ObligationId, AppId, InterfaceId,
                                          ArgType),
                               resolved_by(ObligationId, Evidence))).
obligation_judgment_row(Patterns, Ordinal, ObligationId, AppId, InterfaceId,
                        ArgType, Evidence,
                        judged(Ordinal,
                               obligation(ObligationId, AppId, InterfaceId,
                                          ArgType, Patterns),
                               resolved_by(ObligationId, Evidence))).

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
rewrite_user_template_decl(Instances, enum_decl(Name, Variants0),
                           enum_decl(Name, Variants)) :-
    !,
    rewrite_user_template_enum_variants(Instances, Variants0, Variants).
rewrite_user_template_decl(_, Decl, Decl).

rewrite_user_template_enum_variants(Instances, (Left0 ; Right0),
                                    (Left ; Right)) :-
    !,
    rewrite_user_template_enum_variants(Instances, Left0, Left),
    rewrite_user_template_enum_variants(Instances, Right0, Right).
rewrite_user_template_enum_variants(Instances, Variant0, Variant) :-
    Variant0 =.. [Name | Fields0],
    maplist(rewrite_user_template_enum_field(Instances), Fields0, Fields),
    Variant =.. [Name | Fields].

rewrite_user_template_enum_field(Instances, FieldName:Type0,
                                 FieldName:Type) :-
    rewrite_user_template_type(Instances, Type0, Type).

rewrite_user_template_type(Instances, Type0, Type) :-
    memberchk(Type0, Instances),
    !,
    canonical_type_name(Type0, Type).
rewrite_user_template_type(Instances, annotated_type(Type0, Applications),
                           annotated_type(Type, Applications)) :-
    !,
    rewrite_user_template_type(Instances, Type0, Type).
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
    findall(Type, ( generic_source_type(Decls, Type), generic_type(Type) ),
            Found),
    maplist(check_ground_generic, Found),
    findall(Instance,
            ( member(Type, Found), generic_dependency(Type, Instance) ),
            FoundInstances),
    sort(FoundInstances, Instances).

% Before enum lowering, payload fields live inside enum_decl/2 rather than as
% col_type/3.  Generic applications therefore use the same source walk in
% ordinary columns and enum payloads.
generic_source_type(Decls, Type) :-
    member(col_type(_, _, Type), Decls).
generic_source_type(Decls, Type) :-
    member(enum_decl(_, Variants), Decls),
    enum_payload_type(Variants, Type).
generic_source_type(Decls, Type) :-
    member(compiler_type_apply_request(Type), Decls).

generic_type(list(_)).
generic_type(list_entity_dense_sequence(_)).
generic_type(list_interned_set(_)).
generic_type(list_entity_linked_sequence(_)).
generic_type(option(Type)) :- contains_list_flavor(Type).
generic_type(annotated_type(Type, _)) :- generic_type(Type).

contains_list_flavor(Type) :-
    once(( unwrapped_column_type(Type, Inner), list_flavor(Inner) )).

generic_dependency(option(Type), Instance) :-
    generic_dependency(Type, Instance).
generic_dependency(annotated_type(Type, _), Instance) :-
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
    Artifacts = [ artifact(decl(col_type(Entity, content, text))),
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
