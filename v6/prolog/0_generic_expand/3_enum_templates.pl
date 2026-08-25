% A parameterized enum template mints one concrete enum_decl per ground
% application, with variant payload types substituted. The enum lowering phase
% (which runs after generic expansion) then lowers each concrete enum_decl into
% its tag relation and one relation per variant.
expand_user_enum_templates(Decls0, Instances, Decls) :-
    enum_template_definitions(Decls0, Templates),
    (   Templates == []
    ->  Instances = [],
        Decls = Decls0
    ;   check_enum_template_application_arities(Decls0, Templates),
        enum_template_instances(Decls0, Templates, Found),
        sort(Found, Instances),
        maplist(instantiate_enum_template(Templates), Instances, MintedEnums),
        enum_payload_type_mirrors(MintedEnums, Decls0, PayloadMirrors),
        enum_template_module_rows(Decls0, Instances, ModuleRows),
        append(Decls0, ModuleRows, EnumContext),
        enum_template_type_rows(EnumContext, MintedEnums, EnumRows0),
        enum_template_derived_rows(Decls0, Instances, MintedEnums,
                                    EnumDerivedRows),
        append(EnumRows0, EnumDerivedRows, EnumRows),
        exclude(is_rel_template_enum, Decls0, Kept0),
        maplist(rewrite_user_template_decl(Instances), Kept0, Kept1),
        merge_enum_template_rows(EnumRows, Kept1, Kept2),
        append([Kept2, ModuleRows, PayloadMirrors, MintedEnums], Flat0),
        append(Flat0, [], Decls)
    ).

% A minted enum's variant payload may name a relation (e.g. err(error: L)
% instantiated with L = host_error). The parse-time normalize_relation_value_
% decls mirror only covers relations referenced in SURFACE enum payloads, so a
% payload rel reached only through a minted enum gets its type_decl mirror
% minted here, from the same col_type rows that describe it.
enum_payload_type_mirrors(MintedEnums, Decls, Mirrors) :-
    findall(Type,
            ( member(enum_decl(_, Variants), MintedEnums),
              enum_payload_type(Variants, Type),
              atom(Type),
              \+ scalar_element(Type),
              relation_schema_for(Decls, Type, _) ),
            Found),
    list_to_set(Found, Types),
    findall(type_decl(Type, Specs),
            ( member(Type, Types),
              relation_schema_for(Decls, Type, Specs),
              \+ memberchk(type_decl(Type, _), Decls) ),
            Mirrors).

enum_payload_type(Variants, Type) :-
    enum_variant_term(Variants, Variant),
    Variant =.. [_ | Fields],
    member(_:Type, Fields).

relation_schema_for(Decls, Type, Specs) :-
    findall(col(Column, ColType),
            member(col_type(Type/_, Column, ColType), Decls),
            Specs),
    Specs \== [].

enum_template_definitions(Decls, Templates) :-
    findall(template(Name, Parameters, Variants),
            ( member(rel_template_enum(Segments, Parameters, Variants), Decls),
              atomic_list_concat(Segments, '__', Name) ),
            Templates).

enum_template_instances(Decls, Templates, Instances) :-
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

% Wrong generic arity on an enum application rides the same diagnostic the
% generic-product templates use (generic_template_arity), checked at discovery
% so a malformed application is refused even when no well-formed one exists.
check_enum_template_application_arities(Decls, Templates) :-
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

instantiate_enum_template(Templates, Application, enum_decl(ConcreteName, Variants)) :-
    Application =.. [Name | Arguments],
    memberchk(template(Name, Parameters, Variants0), Templates),
    maplist(template_parameter_name, Parameters, ParameterNames),
    pairs_keys_values(Bindings, ParameterNames, Arguments),
    canonical_type_name(Application, ConcreteName),
    substitute_template_type(Bindings, Variants0, Variants).

enum_template_type_rows(Decls, MintedEnums, Rows) :-
    findall(Row,
            ( member(enum_decl(ConcreteName, Variants), MintedEnums),
              append(Decls, [enum_decl(ConcreteName, Variants)], Context),
              enum_type_rows(Context, EnumRows),
              member(Row, EnumRows) ),
            Found),
    sort(Found, Rows).

enum_template_module_rows(Decls, Instances, Rows) :-
    findall(semantic_decl_module(enum, ConcreteName, ModuleHash),
            ( member(Application, Instances),
              canonical_type_name(Application, ConcreteName),
              Application =.. [Constructor | _],
              member(semantic_decl_module(enum, Constructor, ModuleHash), Decls) ),
            Found),
    sort(Found, Rows).

enum_template_derived_rows(Decls, Instances, MintedEnums, Rows) :-
    findall(derived_from(ConcreteId, ApplicationId),
            ( member(Application, Instances),
              canonical_type_name(Application, ConcreteName),
              member(enum_decl(ConcreteName, _), MintedEnums),
              semantic_decl_id(Decls, enum, ConcreteName, ConcreteId),
              Application =.. [ConstructorName | Arguments],
              semantic_type_constructor_id(Decls, ConstructorName, ConstructorId),
              semantic_application_id(Decls, ConstructorId, Arguments,
                                      ApplicationId) ),
            Found),
    sort(Found, Rows).

merge_enum_template_rows([], Decls, Decls).
merge_enum_template_rows(Rows, Decls0, Decls) :-
    (   memberchk(semantic_type_rows(_), Decls0)
    ->  maplist(merge_one_enum_template_rows(Rows), Decls0, Decls)
    ;   append(Decls0, [semantic_type_rows(Rows)], Decls)
    ).

merge_one_enum_template_rows(EnumRows, semantic_type_rows(Rows0),
                             semantic_type_rows(Rows)) :-
    !,
    append(Rows0, EnumRows, Unsorted),
    sort(Unsorted, Rows).
merge_one_enum_template_rows(_, Decl, Decl).

% Surface enum rows must be present before compiler-plane evaluation so the
% generic graph exposes variant edges during the same immutable round.  The
% later enum phase merges the same canonical rows idempotently.
merge_surface_enum_type_rows(SourceDecls, Decls0, Decls) :-
    enum_type_rows(SourceDecls, Rows),
    merge_enum_template_rows(Rows, Decls0, Decls).
