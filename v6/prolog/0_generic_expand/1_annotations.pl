% Anonymous sums materialize after the source enum row pass has already been
% planned.  Their declaration row exists from anonymous expansion; variant
% declaration/member rows are added here before enum lowering erases the
% source enum_decl/2 term.  Catalog type emitters recover tagged union fields
% from these rows.
merge_anonymous_enum_type_rows(Decls0, Decls) :-
    findall(enum_decl(Name, Variants),
            ( member(anonymous_generated_decl(Name), Decls0),
              member(enum_decl(Name, Variants), Decls0) ),
            AnonymousEnums),
    findall(semantic_decl_module(enum, Name, ModuleHash),
            ( member(anonymous_generated_decl(Name), Decls0),
              member(semantic_decl_module(enum, Name, ModuleHash), Decls0) ),
            AnonymousEnumModules),
    append(AnonymousEnums, AnonymousEnumModules, AnonymousEnumDecls),
    enum_type_rows(AnonymousEnumDecls, EnumRows),
    (   EnumRows == []
    ->  Decls = Decls0
    ;   memberchk(semantic_type_rows(_), Decls0)
    ->  maplist(merge_anonymous_enum_type_rows_(EnumRows), Decls0, Decls)
    ;   append(Decls0, [semantic_type_rows(EnumRows)], Decls)
    ).

merge_anonymous_enum_type_rows_(EnumRows, semantic_type_rows(Rows0),
                                semantic_type_rows(Rows)) :-
    !,
    append(Rows0, EnumRows, Unsorted),
    sort(Unsorted, Rows).
merge_anonymous_enum_type_rows_(_, Decl, Decl).

% The annotation carrier survives generic substitution and anonymous minting.
% This is the first point where the owner/member identity and the concrete
% underlying type are both available.  Execution remains in the next card;
% its input is a typed request IR with the authored carrier intact.
handoff_annotation_requests(Decls0, Decls) :-
    (   annotation_program_has_requests(Decls0)
    ->  findall(Request,
                annotation_member_request(Decls0, Request),
                Requests0)
    ;   Requests0 = []
    ),
    list_to_set(Requests0, Requests),
    ( Requests == []
    -> Decls = Decls0
    ;  append(Decls0, [compiler_annotation_requests(Requests)], Decls)
    ).

annotation_program_has_requests(Decls) :-
    member(col_type(_, _, Type), Decls),
    sub_term(annotated_type(_, Applications), Type),
    Applications \== [],
    !.
annotation_program_has_requests(Decls) :-
    member(col_type(Name/Arity, return, type), Decls),
    member(col_type(Ref, _, Type), Decls),
    Ref \== Name/Arity,
    sub_term(Application, Type),
    compound(Application),
    functor(Application, Name, _),
    !.

% Annotation calls are ordinary compiler-relation queries.  This phase only
% supplies the implicit Target and enforces the annotation-specific signature;
% closure construction remains evaluate_compiler_relations/3.
evaluate_annotation_requests(Decls0, Rules, Bindings, Decls) :-
    (   memberchk(compiler_annotation_requests(Requests), Decls0)
    ->  partition_compiler_program(Decls0, Rules,
                                   compiler_relations(Relations, CompilerRules0),
                                   _, _),
        evaluate_annotation_requests_with_relations(Decls0, Bindings, Requests,
                                                    Relations, CompilerRules0,
                                                    Decls)
    ;   Decls = Decls0
    ).

evaluate_annotation_requests_with_relations(Decls0, Bindings, Requests,
                                             Relations, CompilerRules0, Decls) :-
        elaborate_compiler_rules(Decls0, Bindings, CompilerRules0,
                                 CompilerRules, SeedRows0),
        annotation_type_request_demand_rows(Decls0, Relations, Requests,
                                            DemandRows),
        append(SeedRows0, DemandRows, SeedRows),
        evaluate_compiler_relations(
            compiler_relations(Relations, CompilerRules), SeedRows, Closure),
        evaluate_annotation_request_rows(Decls0, Relations, Closure, Requests,
                                         Evidence, Results),
        rewrite_annotation_declarations(Decls0, Results, Rewritten0),
        bridge_key_annotation_evidence(Rewritten0, Evidence, Bridged),
        ensure_annotation_relation_mirrors(Bridged, Results, Rewritten),
        rewrite_annotation_semantic_rows(Rewritten, Results, Canonicalized),
        append(Canonicalized, [compiler_annotation_evidence(Evidence)], Decls).

annotation_type_request_demand_rows(Decls, Relations, Requests, Rows) :-
    memberchk(compiler_relation(type_requested/3, 3, _), Relations),
    !,
    findall(type_requested(Application, Constructor, Arguments),
            ( member(annotation_request(_, _, _, _,
                                        annotation_steps(_, Steps)), Requests),
              member(annotation_step(_, Input, Application0, _), Steps),
              ground(Input),
              annotation_application_parts(Application0, Name, Arguments0),
              annotation_relation_ref(Decls, Name, Ref),
              findall(Column, member(col_type(Ref, Column, _), Decls), Columns),
              findall(Type, member(col_type(Ref, _, Type), Decls), Types),
              annotation_signature_shape(Ref, Columns, Types),
              compiler_derived_constructor(Decls, Name, _),
              bind_annotation_arguments(Decls, Columns, Types, Input,
                                        Arguments0, RowArguments),
              nth1(ReturnPosition, Columns, return),
              remove_argument_at(ReturnPosition, RowArguments, Arguments),
              ground(Arguments),
              semantic_decl_id(Decls, relation, Name, Constructor),
              Application = application(Constructor, Arguments),
              Ref = Name/_ ),
            Rows0),
    sort(Rows0, Rows).
annotation_type_request_demand_rows(_, _, _, []).

remove_argument_at(Position, Values, Rest) :-
    nth1(Position, Values, _, Rest).

rewrite_annotation_semantic_rows(Decls0, Results, Decls) :-
    maplist(rewrite_annotation_semantic_decl(Results), Decls0, Decls).

rewrite_annotation_semantic_decl(Results, semantic_type_rows(Rows0),
                                 semantic_type_rows(Rows)) :-
    !,
    maplist(rewrite_annotation_semantic_row(Results), Rows0, Rows).
rewrite_annotation_semantic_decl(_, Decl, Decl).

rewrite_annotation_semantic_row(Results,
                                member(MemberId, Owner, Position, Name, _),
                                member(MemberId, Owner, Position, Name,
                                       type_ref(TypeId))) :-
    member(annotation_result(Owner, MemberId, [Name], TypeId), Results),
    !.
rewrite_annotation_semantic_row(_, Row, Row).

evaluate_annotation_request_rows(_, _, _, [], [], []).
evaluate_annotation_request_rows(Decls, Relations, Closure,
                                 [Request | Rest], Evidence, [Result | Results]) :-
    Request = annotation_request(Owner, Member, Site, _,
                                 annotation_steps(Input, Steps)),
    ( Steps == []
    -> StepEvidence = [], Output = Input
    ;  evaluate_annotation_steps(Decls, Relations, Closure, Owner, Member, Site,
                                 Steps, StepEvidence, Output)
    ),
    Evidence = StepEvidenceTail,
    append(StepEvidence, MoreEvidence, StepEvidenceTail),
    Result = annotation_result(Owner, Member, Site, Output),
    evaluate_annotation_request_rows(Decls, Relations, Closure, Rest,
                                     MoreEvidence, Results).

evaluate_annotation_steps(Decls, Relations, Closure, Owner, Member, Site,
                          [annotation_step(Ordinal, Input0, Application0, _) | Rest],
                          Evidence,
                          Final) :-
    Input = Input0,
    annotation_application_parts(Application0, Name, Arguments0),
    annotation_relation_signature(Decls, Relations, Name, Ref, Columns, Types),
    validate_annotation_arguments(Columns, Arguments0),
    bind_annotation_arguments(Decls, Columns, Types, Input, Arguments0, Arguments),
    annotation_output(Decls, Closure, Ref, Columns, Arguments, Output),
    annotation_resolved_application(Decls, Ref, Name, Arguments0, Input, Application),
    annotation_row_arguments(Columns, Arguments, Output, RowArguments),
    AnnotationRow =.. [Name | RowArguments],
    evaluate_annotation_steps_with_input(Decls, Relations, Closure, Owner, Member,
                                         Site, Rest, Output, MoreEvidence, Final),
    Evidence = [annotation_evidence(Member, Site, Ordinal, Input,
                                    Application, Output, AnnotationRow) | MoreEvidence].

evaluate_annotation_steps_with_input(_, _, _, _, _, _, [], Input, [], Input).
evaluate_annotation_steps_with_input(Decls, Relations, Closure, Owner, Member,
                                     Site,
                                     [annotation_step(Ordinal, _, Application0, _) | Rest],
                                     Input,
                                     Evidence,
                                     Final) :-
    annotation_application_parts(Application0, Name, Arguments0),
    annotation_relation_signature(Decls, Relations, Name, Ref, Columns, Types),
    validate_annotation_arguments(Columns, Arguments0),
    bind_annotation_arguments(Decls, Columns, Types, Input, Arguments0, Arguments),
    annotation_output(Decls, Closure, Ref, Columns, Arguments, Output),
    annotation_resolved_application(Decls, Ref, Name, Arguments0, Input, Application),
    annotation_row_arguments(Columns, Arguments, Output, RowArguments),
    AnnotationRow =.. [Name | RowArguments],
    evaluate_annotation_steps_with_input(Decls, Relations, Closure, Owner, Member,
                                         Site, Rest, Output, MoreEvidence, Final),
    Evidence = [annotation_evidence(Member, Site, Ordinal, Input,
                                    Application, Output, AnnotationRow) | MoreEvidence].

annotation_application_parts(Application, Name, Arguments) :-
    ( Application = relation_application(named(_, relation, Name), Arguments)
    -> true
    ; atom(Application) -> Name = Application, Arguments = []
    ; Application =.. [Name | Arguments] ).

annotation_resolved_application(Decls, _Ref, Name, Arguments0, Input,
                                relation_application(RelationId, Arguments)) :-
    semantic_decl_id(Decls, relation, Name, RelationId),
    exclude(annotation_target_argument, Arguments0, ExplicitArguments),
    Arguments = [named('Target', Input) | ExplicitArguments].

annotation_target_argument(named(Name, _)) :- downcase_atom(Name, target).

annotation_row_arguments(Columns, Arguments, Output, RowArguments) :-
    nth1(ReturnPosition, Columns, return),
    same_length(Arguments, RowArguments),
    annotation_row_arguments_(Arguments, ReturnPosition, Output, 1,
                              RowArguments).

annotation_row_arguments_([], _, _, _, []).
annotation_row_arguments_([_ | Rest], ReturnPosition, Output, ReturnPosition,
                          [Output | RowRest]) :-
    !,
    Position is ReturnPosition + 1,
    annotation_row_arguments_(Rest, ReturnPosition, Output, Position, RowRest).
annotation_row_arguments_([Argument | Rest], ReturnPosition, Output, Position,
                          [Argument | RowRest]) :-
    Next is Position + 1,
    annotation_row_arguments_(Rest, ReturnPosition, Output, Next, RowRest).

annotation_relation_ref(Decls, Name, Ref) :-
    member(col_type(Ref, _, _), Decls), Ref = Name/_, !.

annotation_relation_signature(Decls, Relations, Name, Ref, Columns, Types) :-
    ( annotation_relation_ref(Decls, Name, Ref)
    -> true
    ; throw(unsupported_construct(annotation_unknown_relation(Name)) ) ),
    ( memberchk(compiler_relation(Ref, _, _), Relations)
    -> true
    ; throw(unsupported_construct(annotation_not_compiler_relation(Ref)) ) ),
    findall(Column, member(col_type(Ref, Column, _), Decls), Columns),
    findall(Type, member(col_type(Ref, _, Type), Decls), Types),
    annotation_signature_shape(Ref, Columns, Types).

annotation_signature_shape(_Ref, [_First | Rest], [type | Types]) :-
    append(ArgumentColumns, [return], Rest),
    append(ArgumentTypes, [type], Types),
    same_length(ArgumentColumns, ArgumentTypes),
    !.
annotation_signature_shape(Ref, _, _) :-
    throw(unsupported_construct(annotation_invalid_signature(Ref))).

validate_annotation_arguments(Columns, Arguments) :-
    annotation_explicit_columns(Columns, ExplicitColumns),
    findall(Name, member(named(Name, _), Arguments), Named0),
    maplist(downcase_atom, Named0, Named),
    ( duplicate_annotation_keyword(Named, target)
    -> throw(unsupported_construct(annotation_target_is_implicit))
    ; true ),
    ( member(Name, Named), \+ memberchk(Name, [target | ExplicitColumns])
    -> throw(unsupported_construct(annotation_unknown_keyword(Name)))
    ; true ),
    ( duplicate_annotation_keyword(Named, Duplicate)
    -> throw(unsupported_construct(annotation_duplicate_keyword(Duplicate)))
    ; true ),
    findall(Value, member(pos(Value), Arguments), Positionals),
    length(Positionals, PositionalCount),
    length(ExplicitColumns, ExplicitCount),
    ( PositionalCount > ExplicitCount
    -> throw(unsupported_construct(annotation_too_many_positional_arguments))
    ; true ),
    annotation_positional_named_duplicate(ExplicitColumns, Positionals, Named).

annotation_explicit_columns(Columns, ExplicitColumns) :-
    Columns = [_Target | Rest],
    exclude(=(return), Rest, ExplicitColumns0),
    maplist(downcase_atom, ExplicitColumns0, ExplicitColumns).

duplicate_annotation_keyword(Names, Duplicate) :-
    select(Duplicate, Names, Rest), memberchk(Duplicate, Rest), !.

annotation_positional_named_duplicate(Columns, Positionals, Named) :-
    nth1(Position, Positionals, _),
    nth1(Position, Columns, Column),
    memberchk(Column, Named),
    !,
    throw(unsupported_construct(annotation_duplicate_argument(Column))).
annotation_positional_named_duplicate(_, _, _).

bind_annotation_arguments(Decls, Columns, Types, Input, Arguments0, Arguments) :-
    bind_annotation_arguments_(Decls, Columns, Types, Input, Arguments0, 1, Arguments).

bind_annotation_arguments_(_, [], [], _, _, _, []).
bind_annotation_arguments_(Decls, [Column | Columns], [Type | Types], Input,
                            Arguments0, Position, [Value | Values]) :-
    ( Position =:= 1
    -> Value = Input,
       annotation_target_matches(Input, Arguments0)
    ; Column == return
    -> Value = _
    ; annotation_named_or_positional(Column, Position, Arguments0, Raw)
    -> annotation_argument_value(Decls, Type, Raw, Value)
    ; throw(unsupported_construct(annotation_missing_argument(Column)))
    ),
    Next is Position + 1,
    bind_annotation_arguments_(Decls, Columns, Types, Input, Arguments0, Next, Values).

annotation_target_matches(_, Arguments) :-
    findall(Target, (member(named(Name, Target), Arguments),
                     downcase_atom(Name, target)), Targets),
    ( Targets = [_] -> true
    ; Targets = [] -> true
    ; throw(unsupported_construct(annotation_target_is_implicit)) ).

annotation_named_or_positional(Column, _, Arguments, Raw) :-
    member(named(Name, Raw), Arguments), downcase_atom(Name, Lower),
    downcase_atom(Column, Lower), !.
annotation_named_or_positional(_, Position, Arguments, Raw) :-
    findall(Value, member(pos(Value), Arguments), Positional),
    Index is Position - 1,
    nth1(Index, Positional, Raw), !.
annotation_named_or_positional(_, _, Arguments, _) :-
    member(named(Name, _), Arguments),
    throw(unsupported_construct(annotation_unknown_keyword(Name))).

annotation_argument_value(Decls, type, Raw, Value) :- !,
    ( compiler_declared_type_term(Decls, Raw)
    -> semantic_type_id(Decls, Raw, Value)
    ; throw(unsupported_construct(annotation_keyword_type(type, Raw))) ).
annotation_argument_value(_, int, Raw, Raw) :- integer(Raw), !.
annotation_argument_value(_, int, Raw, _) :-
    throw(unsupported_construct(annotation_keyword_type(int, Raw))).
annotation_argument_value(_, text, Raw, Raw) :- atom(Raw), !.
annotation_argument_value(_, text, Raw, _) :-
    throw(unsupported_construct(annotation_keyword_type(text, Raw))).
annotation_argument_value(_, bool, Raw, Raw) :- memberchk(Raw, [true, false]), !.
annotation_argument_value(_, bool, bool_lit(Raw), Raw) :- memberchk(Raw, [true, false]), !.
annotation_argument_value(_, bool, Raw, _) :-
    throw(unsupported_construct(annotation_keyword_type(bool, Raw))).
annotation_argument_value(_, float, Raw, Raw) :- float(Raw), !.
annotation_argument_value(_, float, float_lit(Raw), Raw) :- float(Raw), !.
annotation_argument_value(_, float, Raw, _) :-
    throw(unsupported_construct(annotation_keyword_type(float, Raw))).
annotation_argument_value(_, Type, Raw, _) :-
    throw(unsupported_construct(annotation_keyword_type(Type, Raw))).

annotation_output(Decls, _, Ref, _, Arguments, Output) :-
    memberchk(return_alias(Ref, Position), Decls),
    !,
    nth1(Position, Arguments, Output).
annotation_output(_, Closure, Ref, Columns, Arguments, Output) :-
    nth1(ReturnPosition, Columns, return),
    findall(Value,
            ( member(Row, Closure), annotation_row_ref(Row, Ref), Row =.. [_ | Values],
              annotation_row_matches(Values, Arguments, ReturnPosition),
              nth1(ReturnPosition, Values, Value) ),
            Outputs0),
    sort(Outputs0, Outputs),
    ( Outputs = [Output] -> true
    ; Outputs == [] -> throw(unsupported_construct(annotation_zero_results(Ref)))
    ; throw(unsupported_construct(annotation_multiple_results(Ref, Outputs))) ).

annotation_row_ref(Row, Name/Arity) :-
    compound(Row), functor(Row, Name, Arity).

annotation_row_matches([], [], _).
annotation_row_matches(Values, Arguments, ReturnPosition) :-
    annotation_row_matches(Values, Arguments, ReturnPosition, 1).
annotation_row_matches([], [], _, _).
annotation_row_matches([_ | Rows], [_ | Args], ReturnPosition, ReturnPosition) :-
    !,
    Next is ReturnPosition + 1,
    annotation_row_matches(Rows, Args, ReturnPosition, Next).
annotation_row_matches([Row | Rows], [Arg | Args], ReturnPosition, Position) :-
    Row = Arg,
    Next is Position + 1,
    annotation_row_matches(Rows, Args, ReturnPosition, Next).

rewrite_annotation_declarations(Decls0, Results, Decls) :-
    Results == [],
    !,
    Decls = Decls0.
rewrite_annotation_declarations(Decls0, Results, Decls) :-
    maplist(rewrite_annotation_declaration(Decls0, Results), Decls0, Decls).

rewrite_annotation_declaration(Decls, Results, col_type(Ref, Column, Type0),
                               col_type(Ref, Column, Type)) :- !,
    ref_name(Ref, OwnerName), semantic_decl_id(Decls, relation, OwnerName, Owner),
    member_position(Decls, OwnerName, Column, Position),
    member_id(Owner, Position, Column, Member),
    rewrite_annotation_type(Decls, Results, Owner, Member, [Column], Type0, Type).
rewrite_annotation_declaration(Decls, Results, type_decl(OwnerName, Specs0),
                               type_decl(OwnerName, Specs)) :-
    !,
    semantic_decl_id(Decls, relation, OwnerName, Owner),
    rewrite_annotation_type_specs(Decls, Results, Owner, Specs0, 1, Specs).
rewrite_annotation_declaration(_, _, Decl, Decl).

rewrite_annotation_type_specs(_, _, _, [], _, []).
rewrite_annotation_type_specs(Decls, Results, Owner,
                              [col(Column, Type0) | Rest], Position,
                              [col(Column, Type) | Rewritten]) :-
    member_id(Owner, Position, Column, Member),
    rewrite_annotation_type(Decls, Results, Owner, Member, [Column], Type0,
                            Type),
    Next is Position + 1,
    rewrite_annotation_type_specs(Decls, Results, Owner, Rest, Next,
                                  Rewritten).

rewrite_annotation_type(Decls, Results, Owner, Member, Site,
                        annotated_type(_, _), Type) :-
    member(annotation_result(Owner, Member, Site, TypeId), Results),
    semantic_type_term(Decls, TypeId, Type), !.
rewrite_annotation_type(Decls, Results, Owner, Member, Site,
                        Type0, Type) :-
    member(annotation_result(Owner, Member, Site, TypeId), Results),
    direct_type_application_steps(Decls, Type0, _, [_ | _]),
    semantic_type_term(Decls, TypeId, Type), !.
rewrite_annotation_type(Decls, Results, Owner, Member, Site, Type0, Type) :-
    compound(Type0), Type0 =.. [Name | Arguments0],
    rewrite_annotation_arguments(Decls, Results, Owner, Member, Site,
                                 Arguments0, 1, Arguments),
    Type =.. [Name | Arguments].
rewrite_annotation_type(_, _, _, _, _, Type, Type).

ensure_annotation_relation_mirrors(Decls0, Results, Decls) :-
    findall(Name,
            ( member(annotation_result(_, _, _, named(_, relation, Name)),
                     Results),
              member(col_type(Name/_, _, _), Decls0) ),
            Names0),
    sort(Names0, Names),
    foldl(ensure_annotation_relation_mirror, Names, Decls0, Decls).

ensure_annotation_relation_mirror(Name, Decls0, Decls) :-
    ( memberchk(type_decl(Name, _), Decls0)
    -> Decls = Decls0
    ; findall(col(Column, Type),
              member(col_type(Name/_, Column, Type), Decls0), Specs),
      append(Decls0, [type_decl(Name, Specs)], Decls)
    ).

rewrite_annotation_arguments(_, _, _, _, _, [], _, []).
rewrite_annotation_arguments(Decls, Results, Owner, Member, Site,
                             [Argument0 | Rest], Ordinal, [Argument | Arguments]) :-
    append(Site, [Ordinal], ChildSite),
    rewrite_annotation_type(Decls, Results, Owner, Member, ChildSite,
                            Argument0, Argument),
    Next is Ordinal + 1,
    rewrite_annotation_arguments(Decls, Results, Owner, Member, Site,
                                 Rest, Next, Arguments).

semantic_type_term(_, primitive(Type), Type) :- !.
semantic_type_term(_, named(_, relation, Type), Type) :- !.
semantic_type_term(_, named(_, enum, Type), Type) :- !.
semantic_type_term(Decls, application(Constructor, Arguments), Type) :-
    !,
    semantic_type_constructor_term(Decls, Constructor, Name),
    maplist(semantic_type_term_with_decls(Decls), Arguments, Terms),
    Type =.. [Name | Terms].
semantic_type_term(_, Type, Type).

semantic_type_constructor_term(_, named(_, relation, Name), Name).
semantic_type_constructor_term(_, named(_, enum, Name), Name).

semantic_type_term_with_decls(Decls, TypeId, Type) :-
    semantic_type_term(Decls, TypeId, Type).

bridge_key_annotation_evidence(Decls0, Evidence, Decls) :-
    Evidence == [],
    !,
    Decls = Decls0.
bridge_key_annotation_evidence(Decls0, Evidence, Decls) :-
    reject_nested_key_annotation(Evidence),
    maplist(bridge_key_annotation_declaration(Evidence), Decls0, Decls).

reject_nested_key_annotation(Evidence) :-
    member(annotation_evidence(Member, Site, _, _, Application, _, _), Evidence),
    annotation_application_parts(Application, key, _),
    Site = [_ | Tail], Tail \== [],
    !,
    throw(unsupported_construct(annotation_key_nested_site(Member, Site))).
reject_nested_key_annotation(_).

bridge_key_annotation_declaration(Evidence, col_type(Ref, Column, Type0),
                                  col_type(Ref, Column, Type)) :- !,
    ( annotation_key_site(Evidence, Ref, Column) -> Type = key(Type0) ; Type = Type0 ).
bridge_key_annotation_declaration(_, Decl, Decl).

annotation_key_site(Evidence, Ref, Column) :-
    ref_name(Ref, OwnerName),
    member(annotation_evidence(member(named(_, relation, OwnerName), _, Column),
                               [Column], _, _, Application, _, _), Evidence),
    annotation_application_parts(Application, key, _), !.

annotation_member_request(Decls, Request) :-
    member(col_type(Ref, Name, Type), Decls),
    \+ anonymous_generated_ref(Decls, Ref),
    \+ generated_list_member_ref(Ref),
    ref_name(Ref, OwnerName),
    semantic_decl_id(Decls, relation, OwnerName, OwnerId),
    member_position(Decls, OwnerName, Name, Position),
    member_id(OwnerId, Position, Name, MemberId),
    annotation_type_request(Decls, OwnerId, MemberId, [Name], Type, Request).

% Anonymous declarations are materialized type-expression children, not new
% authored relation members.  Their types are reached from the canonical
% col_type/3 row which caused their minting.
anonymous_generated_ref(Decls, Ref) :-
    ref_name(Ref, Name),
    memberchk(anonymous_generated_decl(Name), Decls).

generated_list_member_ref(Ref) :-
    ref_name(Ref, Name),
    sub_atom(Name, 0, _, _, '__gen__list_').

annotation_type_request(Decls, OwnerId, MemberId, Site,
                        annotated_type(Type, Applications),
                        annotation_request(OwnerId, MemberId, Site,
                                           annotated_type(Type, Applications),
                                           Steps)) :-
    Applications \== [],
    semantic_type_id(Decls, Type, InputTypeId),
    elaborate_annotation(InputTypeId, Applications, Steps).
annotation_type_request(Decls, OwnerId, MemberId, Site,
                        annotated_type(Type, _), Request) :-
    !,
    annotation_type_request(Decls, OwnerId, MemberId, Site, Type, Request).
% A compiler relation ending in `return: type` is callable directly in type
% position.  Its first positional argument supplies the current type; nested
% calls elaborate inside-out into the existing compiler-relation request IR.
annotation_type_request(Decls, OwnerId, MemberId, Site, Type, Request) :-
    direct_type_application_steps(Decls, Type, Input, Applications),
    !,
    semantic_type_id(Decls, Input, InputTypeId),
    elaborate_annotation(InputTypeId, Applications, Steps),
    Request = annotation_request(OwnerId, MemberId, Site,
                                 direct_type_application(Type), Steps).
annotation_type_request(Decls, OwnerId, MemberId, Site, Type, Request) :-
    anonymous_type_members(Decls, Type, Members),
    !,
    member(Segments-ChildType, Members),
    append(Site, Segments, ChildSite),
    annotation_type_request(Decls, OwnerId, MemberId, ChildSite,
                            ChildType, Request).
annotation_type_request(Decls, OwnerId, MemberId, Site, Type, Request) :-
    compound(Type),
    Type =.. [_ | Arguments],
    nth1(Ordinal, Arguments, Argument),
    append(Site, [Ordinal], ChildSite),
    annotation_type_request(Decls, OwnerId, MemberId, ChildSite,
                            Argument, Request).

direct_type_application_steps(Decls, Type, Input, Applications) :-
    compound(Type),
    Type =.. [Name, First | Arguments],
    annotation_relation_ref(Decls, Name, Ref),
    direct_type_relation_signature(Decls, Ref),
    !,
    direct_type_application_steps(Decls, First, Input, Previous),
    Application =.. [Name | Arguments],
    append(Previous, [Application], Applications).
direct_type_application_steps(_, Type, Type, []).

direct_type_relation_signature(Decls, Ref) :-
    findall(Column, member(col_type(Ref, Column, _), Decls), Columns),
    findall(Type, member(col_type(Ref, _, Type), Decls), Types),
    annotation_signature_shape(Ref, Columns, Types).

% The generated name has already replaced the product or sum at this point.
% Recover its immediate fields while preserving the original owner/member.
anonymous_type_members(Decls, Type, Members) :-
    atom(Type),
    memberchk(anonymous_generated_decl(Type), Decls),
    (   member(type_decl(Type, Specs), Decls)
    ->  findall([Name]-ChildType,
                member(col(Name, ChildType), Specs), Members)
    ;   member(enum_decl(Type, Variants), Decls),
        findall([VariantName, Field]-ChildType,
                anonymous_sum_member(Variants, VariantName, Field, ChildType),
                Members)
    ).

anonymous_sum_member((Left ; Right), VariantName, Field, Type) :-
    !,
    ( anonymous_sum_member(Left, VariantName, Field, Type)
    ; anonymous_sum_member(Right, VariantName, Field, Type)
    ).
anonymous_sum_member(Variant, VariantName, Field, Type) :-
    Variant =.. [VariantName | Fields],
    member(Field:Type, Fields).

member_position(Decls, OwnerName, Name, Position) :-
    findall(Column, member(col_type(OwnerName/_, Column, _), Decls), Columns),
    Columns \== [],
    nth1(Position, Columns, Name),
    !.
member_position(Decls, OwnerName, Name, Position) :-
    member(type_decl(OwnerName, Specs), Decls),
    nth1(Position, Specs, col(Name, _)).

% `decode(Parts, [... Part])` over a list(T) source is a keyed read of the
% minted member rel, and becomes that atom for BOTH doors here.
expand_list_decodes(Decls, Rules0, Rules) :-
    (   member(col_type(_, _, Type), Decls), list_flavor(Type)
    ->  maplist(expand_list_decode_rule(Decls), Rules0, Rules)
    ;   Rules = Rules0
    ).

expand_list_decode_rule(Decls, (Head <- Body0), (Head <- Body)) :- !,
    rewrite_list_decodes(Decls, Body0, Body).
expand_list_decode_rule(Decls, (Head <+ Body0), (Head <+ Body)) :- !,
    rewrite_list_decodes(Decls, Body0, Body).
expand_list_decode_rule(_, Rule, Rule).

% An untouched body keeps its ORIGINAL term, never a rebuilt copy: every
% emitted module for a program with no list decode has to stay byte-identical.
rewrite_list_decodes(Decls, Body0, Body) :-
    body_conjunction_goals(Body0, Goals0),
    maplist(rewrite_list_decode(Decls, Goals0), Goals0, Goals),
    (   Goals == Goals0
    ->  Body = Body0
    ;   goals_body_conjunction(Goals, Body)
    ).

rewrite_list_decode(Decls, Goals, Goal0, Goal) :-
    (   nonvar(Goal0), Goal0 = decode(Source, Pattern),
        nonvar(Pattern), Pattern = spread(Element),
        var(Source), ( var(Element) ; atomic(Element) ),
        list_decode_member_ref(Decls, Goals, Source, MemberName)
    ->  Goal =.. [MemberName, Source, _Index, Element]
    ;   Goal = Goal0
    ).

% Variable IDENTITY resolves the source, so the walk is member/2 and never a
% findall: findall copies its template and every source would read unbound.
list_decode_member_ref(Decls, Goals, Source, MemberName) :-
    member(Atom, Goals),
    compound(Atom),
    functor(Atom, Name, Arity),
    Atom =.. [_ | Args],
    nth1(Position, Args, Argument),
    Argument == Source,
    findall(Type, member(col_type(Name/Arity, _, Type), Decls), ColumnTypes),
    length(ColumnTypes, Arity),
    nth1(Position, ColumnTypes, list(ElementType)),
    !,
    canonical_type_name(list(ElementType), EntityName),
    atomic_list_concat([EntityName, member], '__', MemberName).

body_conjunction_goals(Body, Goals) :-
    (   nonvar(Body), Body = (Left, Right)
    ->  body_conjunction_goals(Left, LeftGoals),
        body_conjunction_goals(Right, RightGoals),
        append(LeftGoals, RightGoals, Goals)
    ;   Goals = [Body]
    ).

goals_body_conjunction([Goal], Goal) :- !.
goals_body_conjunction([Goal | Rest], (Goal, More)) :-
    goals_body_conjunction(Rest, More).
