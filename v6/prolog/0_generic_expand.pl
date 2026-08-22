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
            generic_type_ir/2,
            freeze_type_rows/2,
            normalize_key_wrappers/2,
            schema_member_rows/2,
            compiler_type_source_rows/3,
            type_relation_rows/2,
            schema_member_transport_rows/3,
            expand_generic_program_with_bindings/3,
            reset_type_row_memo/0
          ]).

:- use_module(library(apply)).
:- use_module(library(assoc)).
:- use_module(library(pairs), [group_pairs_by_key/2]).
:- use_module(library(crypto)).
:- use_module(library(lists)).
:- use_module('compile/0_trace', [run_compile_step/4]).
:- use_module('0_option_expand', [expand_option_decls/2, scalar_element/1]).
:- use_module('0_enum_expand', [enum_type_rows/2]).
:- use_module('0_type_plane', [unwrapped_column_type/2]).
:- use_module('0_anonymous_expand', [expand_anonymous_decls/2]).
:- use_module('0_annotation_expand', [elaborate_annotation/3]).
:- use_module('0_type_ids',
              [ decl_id/4, primitive_id/2, param_id/4, member_id/4,
                constraint_id/3, impl_id/3, app_id/3, arg_id/3,
                id_kind_name/3 ]).
:- use_module('0_compiler_relations',
              [ partition_compiler_program/5,
                evaluate_compiler_relations/3,
                compiler_type_apply_requests/3 ]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- discontiguous replace_generic_type/3.
:- discontiguous generated_decl_module/4.

expand_generic_in_context(expansion_context(_, Bindings), Program, Expanded) :-
    !,
    expand_generic_program_with_bindings(Program, Bindings, Expanded).
expand_generic_in_context(_, Program, Expanded) :-
    expand_generic_program(Program, Expanded).

expand_generic_program(Program, Expanded) :-
    expand_generic_program_with_bindings(Program, [], Expanded).

expand_generic_program_with_bindings(prog(Decls0, Rules0), Bindings,
                                     Expanded) :-
    type_apply_refreeze(Decls0, Rules0, Bindings, [], none, 0, Expanded).

type_apply_refreeze(Decls0, Rules0, Bindings, Seen0, PreviousRows, Round,
                    prog(Decls, Rules)) :-
    ( Round >= 16
    -> throw(unsupported_construct(type_apply_round_limit_exhausted(16)))
    ; true
    ),
    expand_generic_program_round(prog(Decls0, Rules0), Bindings,
                                 prog(RoundDecls, RoundRules)),
    canonical_semantic_type_rows(RoundDecls, CurrentRows),
    type_apply_requests(Decls0, RoundDecls, Requests),
    subtract(Requests, Seen0, NewRequests),
    ( NewRequests == [], PreviousRows == CurrentRows
    -> erase_type_apply_transport(RoundDecls, Decls),
       Rules = RoundRules
    ; append(Decls0, NewRequests, NextDecls),
      append(Seen0, NewRequests, Seen1),
      NextRound is Round + 1,
      type_apply_refreeze(NextDecls, Rules0, Bindings, Seen1, CurrentRows,
                          NextRound, prog(Decls, Rules))
    ).

canonical_semantic_type_rows(Decls, Rows) :-
    findall(Row,
            ( member(semantic_type_rows(SourceRows), Decls),
              member(Row, SourceRows) ),
            Rows0),
    sort(Rows0, Rows).

erase_type_apply_transport(Decls0, Decls) :-
    exclude(type_apply_transport_decl, Decls0, Decls).

type_apply_transport_decl(compiler_type_apply_request_rows(_)).
type_apply_transport_decl(compiler_type_apply_request(_)).

type_apply_requests(SourceDecls, Decls, Requests) :-
    member(compiler_type_apply_request_rows(RequestRows), Decls),
    !,
    findall(compiler_type_apply_request(Type),
            ( member(type_apply_request(Application), RequestRows),
              Application = application(Constructor, _),
              \+ frozen_type_application(Decls, Application, Constructor),
              type_apply_request_type(SourceDecls, Constructor, Application, Type),
              ground(Type) ),
            Requests0),
    sort(Requests0, Requests).
type_apply_requests(_, _, []).

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

expand_generic_program_round(prog(Decls0, Rules0), Bindings,
                             prog(Decls, Rules)) :-
    Step = run_compile_step(plan),
    call(Step, generic:expand_user_templates,
         expand_user_templates(Decls0, Rules0, _UserInstances, UserDecls), _),
    call(Step, generic:expand_user_enum_templates,
         expand_user_enum_templates(UserDecls, _EnumInstances, WithEnumDecls), _),
    call(Step, generic:generic_fixpoint,
         generic_fixpoint(WithEnumDecls, Instances, WithMintedDecls), _),
    call(Step, generic:validate_generated_name_collisions,
         validate_generated_name_collisions(UserDecls, Rules0, Instances), _),
    call(Step, generic:expand_list_decodes,
         expand_list_decodes(WithMintedDecls, Rules0, ExpandedRules), _),
    call(Step, generic:replace_generic_types,
         replace_generic_types(WithMintedDecls, Instances, RewrittenDecls), _),
    call(Step, generic:expand_anonymous_decls,
         expand_anonymous_decls(RewrittenDecls, AnonymousDecls), _),
    call(Step, generic:handoff_annotation_requests,
         handoff_annotation_requests(AnonymousDecls, AnnotationHandedOffDecls), _),
    call(Step, generic:merge_anonymous_enum_type_rows,
         merge_anonymous_enum_type_rows(AnnotationHandedOffDecls,
                                        AnonymousEnumRowedDecls), _),
    call(Step, generic:evaluate_annotation_requests,
         evaluate_annotation_requests(AnonymousEnumRowedDecls, ExpandedRules,
                                      Bindings, AnnotationEvaluatedDecls), _),
    call(Step, generic:normalize_key_wrappers,
         normalize_key_wrappers(AnnotationEvaluatedDecls, KeyNormalizedDecls), _),
    call(Step, generic:generic_artifact_order,
         generic_artifact_order(Instances, KeyNormalizedDecls, CanonicalDecls), _),
    call(Step, generic:merge_flavor_type_rows,
         merge_flavor_type_rows(Instances, CanonicalDecls, FlavorRowedDecls), _),
    call(Step, generic:expand_option_decls,
         expand_option_decls(FlavorRowedDecls, OptionDecls), _),
    call(Step, generic:retarget_type_decl_mirrors,
         retarget_type_decl_mirrors(OptionDecls, ExpandedDecls), _),
    call(Step, generic:freeze_type_rows,
         freeze_type_rows(ExpandedDecls, FrozenExpandedDecls), _),
    call(Step, generic:elaborate_and_erase_compiler_relations,
         elaborate_and_erase_compiler_relations(FrozenExpandedDecls,
                                                ExpandedRules, Bindings,
                                                Decls, Rules), _).

% Executable comparison arm, written as a second path so the template and
% replacement logic cannot drift apart from the wired entry above.
expand_generic_program_raw(prog(Decls0, Rules0), prog(Decls, Rules)) :-
    expand_user_templates(Decls0, Rules0, _UserInstances, UserDecls),
    expand_user_enum_templates(UserDecls, _EnumInstances, WithEnumDecls),
    generic_fixpoint(WithEnumDecls, Instances, WithMintedDecls),
    validate_generated_name_collisions(UserDecls, Rules0, Instances),
    expand_list_decodes(WithMintedDecls, Rules0, ExpandedRules),
    replace_generic_types(WithMintedDecls, Instances, RewrittenDecls),
    expand_anonymous_decls(RewrittenDecls, AnonymousDecls),
    handoff_annotation_requests(AnonymousDecls, AnnotationHandedOffDecls),
    merge_anonymous_enum_type_rows(AnnotationHandedOffDecls, AnonymousEnumRowedDecls),
    evaluate_annotation_requests(AnonymousEnumRowedDecls, ExpandedRules, [],
                                 AnnotationEvaluatedDecls),
    normalize_key_wrappers(AnnotationEvaluatedDecls, KeyNormalizedDecls),
    generic_artifact_order(Instances, KeyNormalizedDecls, CanonicalDecls),
    merge_flavor_type_rows(Instances, CanonicalDecls, FlavorRowedDecls),
    expand_option_decls(FlavorRowedDecls, OptionDecls),
    retarget_type_decl_mirrors(OptionDecls, ExpandedDecls),
    freeze_type_rows(ExpandedDecls, FrozenExpandedDecls),
    elaborate_and_erase_compiler_relations(FrozenExpandedDecls, ExpandedRules, [],
                                           Decls, Rules).

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
    enum_type_rows(AnonymousEnums, EnumRows),
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
        evaluate_compiler_relations(
            compiler_relations(Relations, CompilerRules), SeedRows0, Closure),
        evaluate_annotation_request_rows(Decls0, Relations, Closure, Requests,
                                         Evidence, Results),
        rewrite_annotation_declarations(Decls0, Results, Rewritten0),
        bridge_key_annotation_evidence(Rewritten0, Evidence, Bridged),
        ensure_annotation_relation_mirrors(Bridged, Results, Rewritten),
        rewrite_annotation_semantic_rows(Rewritten, Results, Canonicalized),
        append(Canonicalized, [compiler_annotation_evidence(Evidence)], Decls).

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
       erase_annotation_transport(RuntimeDecls, RuntimeDecls1, AnnotationEvidence),
       ( AnnotationEvidence == []
       -> Metadata = compiler_type_metadata(MetadataRows, ClosureRows)
       ;  Metadata = compiler_type_metadata(MetadataRows, ClosureRows,
                                            AnnotationEvidence)
       ),
       append(RuntimeDecls1, [Metadata,
                              compiler_type_apply_request_rows(RequestRows)], Decls),
       Rules = RuntimeRules
    ).

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
    elaborate_compiler_atom(Decls, Bindings, Head, Head1),
    elaborate_compiler_body(Decls, Bindings, Body0, Body).
elaborate_compiler_rule(Decls, Bindings, (Head <+ Body0), (Head1 <+ Body)) :-
    !,
    elaborate_compiler_atom(Decls, Bindings, Head, Head1),
    elaborate_compiler_body(Decls, Bindings, Body0, Body).

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

elaborate_compiler_argument(Decls, Bindings, Domain0, Argument, Elaborated) :-
    compiler_argument_domain(Domain0, Domain),
    Domain \== Domain0,
    !,
    elaborate_compiler_argument(Decls, Bindings, Domain, Argument, Elaborated).
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
            Rows0),
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
    compiler_type_constructor(Decls, Constructor),
    maplist(compiler_declared_type_term(Decls), Arguments).

compiler_type_constructor(_, option).
compiler_type_constructor(_, json_list).
compiler_type_constructor(_, list).
compiler_type_constructor(Decls, Constructor) :-
    member(rel_template(Segments, _, _), Decls),
    atomic_list_concat(Segments, '__', Constructor).
compiler_type_constructor(Decls, Constructor) :-
    member(semantic_type_rows(Rows), Decls),
    member(declaration(_, _, Constructor, relation, compile_time), Rows).

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

% Source terms are accepted here but generic_rel/interface/implementation terms
% are never constructed; ids derive from labels and ordinals, not source order.
generic_type_ir(Decls, Rows) :-
    normalized_type_rows(Decls, Rows).

%! schema_member_rows(+Decls, -Rows) is det.
%
% This is a parallel compiler metadata view.  It deliberately stays out of
% semantic_type_rows/1 so the existing runtime declaration term and all
% ordinary type artifacts remain byte-stable while catalog/typegen consumers
% can request the richer role-bearing schema.
schema_member_rows(Decls, Rows) :-
    canonical_member_rows(Decls, MemberRows),
    findall(Row,
            ( member(member(MemberId, OwnerId, Position, Name, TypeRef),
                     MemberRows),
              canonical_member_type_term(Decls, TypeRef, AuthoredType),
              schema_value_type_id(Decls, TypeRef, ValueTypeId),
              schema_member_roles(Decls, OwnerId, Position, Name, TypeRef,
                                  Roles),
              Row = schema_member(MemberId, OwnerId, Position, Name,
                                  AuthoredType, ValueTypeId, Roles) ),
            Unsorted),
    sort(Unsorted, Rows).

canonical_member_rows(Decls, Rows) :-
    member(semantic_type_rows(SemanticRows), Decls),
    !,
    findall(member(MemberId, OwnerId, Position, Name, TypeRef),
            member(member(MemberId, OwnerId, Position, Name, TypeRef),
                   SemanticRows),
            Rows).
canonical_member_rows(Decls, Rows) :-
    normalized_member_rows(Decls, Rows).

canonical_member_type_term(_, type_ref(parameter(ParameterId)), ParameterId) :- !.
canonical_member_type_term(_, type_ref(primitive(Type)), Type) :- !.
canonical_member_type_term(_, type_ref(named(Type)), Type) :- !.
canonical_member_type_term(Decls, type_ref(TypeId), Type) :-
    semantic_type_term(Decls, TypeId, Type).

%! type_relation_rows(+Decls, -Rows) is det.
%  Rows is the concatenation of normalized schema_member/7 and
%  type_relation/5 terms.  A relation without a Self member remains an
%  ordinary relation row and does not enter trait validation.
type_relation_rows(Decls, Rows) :-
    schema_member_rows(Decls, MemberRows),
    findall(Row,
            type_relation_row(Decls, MemberRows, Row),
            RelationRows),
    compiler_metadata_rows(Decls, MetadataRows),
    append([MemberRows, RelationRows, MetadataRows], Rows0),
    sort(Rows0, Rows).

compiler_metadata_rows(Decls, Rows) :-
    member(compiler_type_metadata(MetadataRows, ClosureRows, _), Decls),
    compiler_evidence_rows(Decls, MetadataRows, ClosureRows, EvidenceRows),
    Rows = EvidenceRows,
    !.
compiler_metadata_rows(Decls, Rows) :-
    member(compiler_type_metadata(MetadataRows, ClosureRows), Decls),
    compiler_evidence_rows(Decls, MetadataRows, ClosureRows, EvidenceRows),
    Rows = EvidenceRows,
    !.
compiler_metadata_rows(_, []).

% Evidence is carried beside the target-independent relation metadata so the
% type artifact doors can make the same impl-emission decision as the
% compiler.  One row names the relation owner and carries one complete,
% ground compiler-plane fact.  A fact is complete when its functor is the
% owner's relation name; arity and keyed-position validation remain the
% emitter's responsibility because the relation row owns that contract.
compiler_evidence_rows(Decls, MetadataRows, ClosureRows, EvidenceRows) :-
    findall(type_relation_evidence(OwnerId, Evidence),
            ( member(type_relation(OwnerId, _Self, _Inputs, _Return, _Keys),
                     MetadataRows),
              relation_owner_name(OwnerId, OwnerName),
              compiler_evidence_owner_unambiguous(Decls, OwnerName),
              compiler_evidence_arity(MetadataRows, OwnerId, Arity),
              member(Evidence, ClosureRows),
              compound(Evidence),
              functor(Evidence, OwnerName, Arity),
              ground(Evidence) ),
            Unsorted),
    sort(Unsorted, EvidenceRows).

% Compiler closure terms carry a surface functor, while catalog identity is
% module-qualified.  A name declared by more than one loaded module has no
% source-module field left on that closure term, so it cannot authorize an
% owner-specific Rust impl.  Suppress that evidence until the compiler-plane
% evaluator transports the owner identity itself.
compiler_evidence_owner_unambiguous(Decls, OwnerName) :-
    findall(ModuleHash, member(rel_module_decl(OwnerName, ModuleHash), Decls),
            Hashes0),
    sort(Hashes0, Hashes),
    ( Hashes = [] ; Hashes = [_] ).

compiler_evidence_arity(MetadataRows, OwnerId, Arity) :-
    findall(MemberId,
            member(schema_member(MemberId, OwnerId, _, _, _, _, _),
                   MetadataRows),
            MemberIds),
    length(MemberIds, Arity).

relation_owner_name(named(_, relation, Name), Name) :- !.
relation_owner_name(relation(Name), Name).

metadata_relation_owner_identity(named(ModuleId, relation, Name), ModuleId,
                                 Name) :- !.
metadata_relation_owner_identity(relation(Name), local, Name).

%! schema_member_transport_rows(+CatalogRows, +RelationRows, -Rows) is det.
%  Add typed child rows and the catalog-column bridge used at artifact
%  boundaries.  The normalized schema_member/7 and type_relation/5 terms stay
%  internal; these rows carry no Prolog list-valued columns.
schema_member_transport_rows(CatalogRows, RelationRows, Rows) :-
    findall(schema_member_column(ColumnId, MemberId),
            schema_member_catalog_column(CatalogRows, RelationRows,
                                         MemberId, ColumnId),
            ColumnRows0),
    sort(ColumnRows0, ColumnRows),
    findall(schema_member_role(MemberId, Ordinal, Role, Argument),
            schema_member_role_row(RelationRows, MemberId, Ordinal, Role,
                                   Argument),
            RoleRows),
    findall(type_relation_input(OwnerId, Ordinal, MemberId),
            ( member(type_relation(OwnerId, _, InputMemberIds, _, _),
                     RelationRows),
              nth1(Ordinal, InputMemberIds, MemberId) ),
            InputRows),
    findall(type_relation_key(OwnerId, Ordinal, MemberId),
            ( member(type_relation(OwnerId, _, _, _, KeyMemberIds),
                     RelationRows),
              nth1(Ordinal, KeyMemberIds, MemberId) ),
            KeyRows),
    findall(type_relation_owner(OwnerId, ModuleId, Name),
            ( member(type_relation(OwnerId, _, _, _, _), RelationRows),
              metadata_relation_owner_identity(OwnerId, ModuleId, Name) ),
            OwnerRows0),
    sort(OwnerRows0, OwnerRows),
    append([ColumnRows, RoleRows, InputRows, KeyRows, OwnerRows],
           Rows0),
    sort(Rows0, Rows).

schema_member_catalog_column(CatalogRows, RelationRows, MemberId, ColumnId) :-
    member(schema_member(MemberId, OwnerId, Position, _, _, _, _),
           RelationRows),
    id_kind_name(OwnerId, relation, OwnerName),
    member(row(RelId, _, _, OwnerName, rel, _, _, _, _, _, _), CatalogRows),
    member(row(ColumnId, RelId, Ordinal, _, column, _, _, _, _, _, _),
           CatalogRows),
    Position is Ordinal + 1.

schema_member_role_row(RelationRows, MemberId, Ordinal, Role, Argument) :-
    member(schema_member(MemberId, _, _, _, _, _, Roles), RelationRows),
    nth1(Ordinal, Roles, RoleTerm),
    transport_role(RoleTerm, Role, Argument).

transport_role(self_subject, self_subject, '') :- !.
transport_role(key, key, '') :- !.
transport_role(return, return, '') :- !.
transport_role(anonymous_owner(Path), anonymous_owner, Path).

normalized_member_rows(Decls, Rows) :-
    findall(Row, normalized_member_row(Decls, Row), Rows0),
    first_member_row_per_id(Rows0, Rows).

authored_member_type(Decls, OwnerId, Position, Type) :-
    id_kind_name(OwnerId, relation, OwnerName),
    owner_member_carrier(Decls, OwnerName, carrier(Kind, Specs, _)),
    carrier_member_type(Kind, Position, Specs, Type).

carrier_member_type(template, Position, Specs, Type) :-
    nth1(Position, Specs, column(_, Type)).
carrier_member_type(type_decl, Position, Specs, Type) :-
    nth1(Position, Specs, col(_, Type)).
carrier_member_type(pairs, Position, Pairs, Type) :-
    nth1(Position, Pairs, _-Type).

% Keyed by owner, not by member. Same cache scope as semantic_decl_id/4, which
% is what makes one Decls list per scope safe to assume.
owner_member_carrier(_Decls, OwnerName, Carrier) :-
    atom(OwnerName),
    nb_current(generic_semantic_id_cache, cache(_, _, OwnerIndex)),
    OwnerIndex \== none,
    !,
    (   get_assoc(OwnerName, OwnerIndex, Carrier)
    ->  true
    ;   Carrier = carrier(pairs, [], [])
    ).
owner_member_carrier(Decls, OwnerName, Carrier) :-
    owner_member_carrier_uncached(Decls, OwnerName, Carrier).

%! owner_carrier_index(+Decls, -Index) is det.
% Four Decls passes for the whole program, where the per-owner form below is
% four passes per owner. `none` when a declaration names its owner with an
% unbound term, which get_assoc/3 cannot answer the way member/2 does.
owner_carrier_index(Decls, Index) :-
    findall(Name-template(Specs),
            ( member(rel_template(Segments, _, Specs), Decls),
              atomic_list_concat(Segments, '__', Name) ),
            TemplateRows),
    findall(Name-type_decl(Specs),
            member(type_decl(Name, Specs), Decls),
            TypeDeclRows),
    findall(Name-column(ColumnName, ColumnType),
            member(col_type(Name/_, ColumnName, ColumnType), Decls),
            ColumnRows),
    findall(Name-key_positions(Positions),
            member(keyed(Name/_, Positions), Decls),
            KeyedRows),
    append([TemplateRows, TypeDeclRows, ColumnRows, KeyedRows], Rows),
    (   forall(member(Name0-_, Rows), atom(Name0))
    ->  keysort(Rows, Sorted),
        group_pairs_by_key(Sorted, Grouped),
        maplist(owner_carrier_entry, Grouped, Entries),
        list_to_assoc(Entries, Index)
    ;   Index = none
    ).

% keysort/2 is stable, so each owner's items keep the order the scans they
% replace read them in, and the first template or type_decl still wins.
owner_carrier_entry(Name-Items, Name-carrier(Kind, Specs, Keyed)) :-
    (   memberchk(template(TemplateSpecs), Items)
    ->  Kind = template, Specs = TemplateSpecs
    ;   memberchk(type_decl(DeclSpecs), Items)
    ->  Kind = type_decl, Specs = DeclSpecs
    ;   Kind = pairs,
        findall(ColumnName-ColumnType,
                member(column(ColumnName, ColumnType), Items), Specs)
    ),
    findall(Positions, member(key_positions(Positions), Items), Keyed).

owner_member_carrier_uncached(Decls, OwnerName, carrier(Kind, Specs, Keyed)) :-
    (   member(rel_template(Segments, _, TemplateSpecs), Decls),
        atomic_list_concat(Segments, '__', OwnerName)
    ->  Kind = template, Specs = TemplateSpecs
    ;   member(type_decl(OwnerName, DeclSpecs), Decls)
    ->  Kind = type_decl, Specs = DeclSpecs
    ;   Kind = pairs,
        findall(ColumnName-ColumnType,
                member(col_type(OwnerName/_, ColumnName, ColumnType), Decls),
                Specs)
    ),
    findall(Positions, member(keyed(OwnerName/_, Positions), Decls), Keyed).

schema_value_type_id(_Decls, type_ref(parameter(ParameterId)), ParameterId) :-
    !.
schema_value_type_id(_Decls, type_ref(declaration(TypeId)), TypeId) :- !.
schema_value_type_id(_Decls, type_ref(application(TypeId)), TypeId) :- !.
schema_value_type_id(Decls, type_ref(primitive(Type)), TypeId) :-
    !,
    ( Type == type
    -> primitive_id(type, TypeId)
    ;  semantic_type_id(Decls, Type, TypeId)
    ).
schema_value_type_id(Decls, type_ref(named(Type)), TypeId) :-
    !,
    ( Type == type
    -> primitive_id(type, TypeId)
    ;  semantic_type_id(Decls, Type, TypeId)
    ).
schema_value_type_id(Decls, Type, TypeId) :-
    semantic_type_id(Decls, Type, TypeId).

normalized_member_role_row(Decls, member_role(MemberId, Role)) :-
    normalized_member_row(Decls,
                          member(MemberId, OwnerId, Position, Name, TypeRef)),
    schema_member_roles_from_carriers(Decls, OwnerId, Position, Name, TypeRef,
                                      Roles),
    member(Role, Roles).

schema_member_roles(Decls, OwnerId, Position, Name, TypeRef, Roles) :-
    MemberId = member(OwnerId, Position, Name),
    (   member(semantic_type_rows(SemanticRows), Decls),
        findall(Role,
                member(member_role(MemberId, Role), SemanticRows),
                CanonicalRoles),
        CanonicalRoles \== []
    ->  sort(CanonicalRoles, Roles)
    ;   schema_member_roles_from_carriers(Decls, OwnerId, Position, Name,
                                          TypeRef, Roles)
    ).

schema_member_roles_from_carriers(Decls, OwnerId, Position, Name, TypeRef,
                                  Roles) :-
    ( Name == 'Self' -> SelfRoles = [self_subject] ; SelfRoles = [] ),
    ( owner_key_position(Decls, OwnerId, Position)
    -> KeyRoles = [key]
    ;  KeyRoles = []
    ),
    ( Name == return -> ReturnRoles = [return] ; ReturnRoles = [] ),
    ( ( anonymous_owner_path_for_member(Decls, OwnerId, Position, Path)
      ; anonymous_owner_path(TypeRef, Path)
      )
    -> AnonymousRoles = [anonymous_owner(Path)]
    ;  AnonymousRoles = []
    ),
    append([SelfRoles, KeyRoles, ReturnRoles, AnonymousRoles], RawRoles),
    list_to_set(RawRoles, Roles).

anonymous_owner_path_for_member(Decls, OwnerId, Position, Path) :-
    authored_member_type(Decls, OwnerId, Position, Type),
    anonymous_owner_source_path(Type, Path).

anonymous_owner_source_path(anonymous_product(Path, _), Path) :- !.
anonymous_owner_source_path(anonymous_sum(Path, _), Path) :- !.
anonymous_owner_source_path(anonymous(Path, _), Path) :- !.

owner_key_position(Decls, OwnerId, Position) :-
    id_kind_name(OwnerId, relation, OwnerName),
    owner_member_carrier(Decls, OwnerName, carrier(_, _, KeyedPositions)),
    member(Positions, KeyedPositions),
    memberchk(Position, Positions).

anonymous_owner_path(anonymous_product(Path, _), Path) :- !.
anonymous_owner_path(anonymous_sum(Path, _), Path) :- !.
anonymous_owner_path(anonymous(Path, _), Path) :- !.
anonymous_owner_path(type_ref(named(Type)), Path) :-
    !,
    anonymous_owner_path(Type, Path).

type_relation_row(_Decls, MemberRows,
                  type_relation(OwnerId, SelfMemberId, InputMemberIds,
                                ReturnMemberOrNone, KeyMemberIds)) :-
    setof(Owner,
          MemberId^Position^Name^AuthoredType^ValueType^Roles^
          member(schema_member(MemberId, Owner, Position, Name,
                                AuthoredType, ValueType, Roles), MemberRows),
          Owners),
    member(OwnerId, Owners),
    findall(Position-MemberId-Name-Type,
            member(schema_member(MemberId, OwnerId, Position, Name, Type,
                                  _, _), MemberRows), MemberPairs0),
    sort(MemberPairs0, MemberPairs),
    self_members(MemberPairs, SelfMembers),
    self_member_or_none(SelfMembers, SelfMemberId),
    input_member_ids(MemberPairs, InputMemberIds),
    return_member_or_none(MemberPairs, ReturnMemberOrNone),
    key_member_ids(MemberRows, OwnerId, KeyMemberIds).

self_members(MemberPairs, SelfMembers) :-
    findall(Position-MemberId-Type,
            member(Position-MemberId-'Self'-Type, MemberPairs), SelfMembers).

self_member_or_none([], none).
self_member_or_none([_-MemberId-_|_], MemberId).

input_member_ids(MemberPairs, InputMemberIds) :-
    findall(MemberId,
            ( member(_-MemberId-Name-_, MemberPairs),
              Name \== 'Self', Name \== return ),
            InputMemberIds).

return_member_or_none(MemberPairs, MemberId) :-
    ( member(_-MemberId-return-_, MemberPairs) -> true ; MemberId = none ).

key_member_ids(MemberRows, OwnerId, KeyMemberIds) :-
    findall(MemberId,
            ( member(schema_member(MemberId, OwnerId, _, _, _, _, Roles),
                     MemberRows),
              memberchk(key, Roles) ),
            KeyMemberIds).

% One compile asks for the same rows five times: expand_user_templates/4 and
% freeze_type_rows/2 each rebuild them, the generic pipeline runs twice, and
% expand_program_run/4 freezes once more. Keyed by variant, so an entry is
% only ever reused for a Decls list that would rebuild the same rows.
:- thread_local type_row_memo/3.

reset_type_row_memo :-
    retractall(type_row_memo(_, _, _)).

normalized_type_rows(Decls, Rows) :-
    variant_sha1(Decls, Hash),
    (   type_row_memo(Hash, MemoDecls, MemoRows),
        MemoDecls =@= Decls
    ->  Rows = MemoRows
    ;   normalized_type_rows_rebuilt(Decls, Rows),
        % A non-ground row set would lose its variable identity to assertz/1.
        (   ground(Rows)
        ->  assertz(type_row_memo(Hash, Decls, Rows))
        ;   true
        )
    ).

normalized_type_rows_rebuilt(Decls, Rows) :-
    owner_carrier_index(Decls, OwnerIndex),
    setup_call_cleanup(
        nb_setval(generic_semantic_id_cache, cache(t, t, OwnerIndex)),
        normalized_type_rows_cached(Decls, Rows),
        nb_delete(generic_semantic_id_cache)).

normalized_type_rows_cached(Decls, Rows) :-
    findall(Row, normalized_declaration_row(Decls, Row), DeclarationRows),
    findall(Row, normalized_parameter_row(Decls, Row), ParameterRows),
    findall(Row, normalized_member_row(Decls, Row), MemberRows0),
    first_member_row_per_id(MemberRows0, MemberRows),
    findall(Row, normalized_member_role_row(Decls, Row), MemberRoleRows),
    findall(Row, normalized_constraint_row(Decls, Row), ConstraintRows),
    normalized_application_rows(Decls, ApplicationRows),
    findall(Row, normalized_implementation_row(Decls, Row), ImplementationRows),
    normalized_derivation_rows(Decls, ApplicationRows, DerivationRows),
    append([DeclarationRows, ParameterRows, MemberRows, MemberRoleRows,
            ConstraintRows,
            ApplicationRows, ImplementationRows, DerivationRows], Unsorted),
    sort(Unsorted, Rows).

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
canonical_type_row_identity(implementation(Id, _, _), implementation(Id)) :- !.
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

normalized_implementation_row(Decls,
                              implementation(Id, Subject, interface_application(InterfaceId))) :-
    member(rel_is_implementation(Ref, Applications), Decls),
    ref_name(Ref, SubjectName),
    semantic_decl_id(Decls, relation, SubjectName, Subject),
    member(Application, Applications),
    interface_application_parts(Application, InterfaceName, []),
    semantic_decl_id(Decls, interface, InterfaceName, InterfaceId),
    impl_id(Subject, InterfaceId, Id).

normalized_implementation_row(Decls,
                              implementation(Id, Subject,
                                            interface_application(InterfaceId,
                                                                  Arguments))) :-
    member(rel_is_implementation(Ref, Applications), Decls),
    ref_name(Ref, SubjectName),
    semantic_decl_id(Decls, relation, SubjectName, Subject),
    member(Application, Applications),
    interface_application_parts(Application, InterfaceName, Arguments),
    Arguments \== [],
    semantic_decl_id(Decls, interface, InterfaceName, InterfaceId),
    semantic_application_id(Decls, InterfaceId, Arguments, InterfaceApplicationId),
    impl_id(Subject, InterfaceApplicationId, Id).

member_constraint_row(Rows, ParameterId, InterfaceId, []) :-
    member(constraint(_, ParameterId, InterfaceId), Rows).
member_constraint_row(Rows, ParameterId, InterfaceId, Patterns) :-
    member(constraint(_, ParameterId, InterfaceId, Patterns), Rows).

member_implementation_row(Rows, Id, Subject, InterfaceId, []) :-
    member(implementation(Id, Subject, interface_application(InterfaceId)), Rows).
member_implementation_row(Rows, Id, Subject, InterfaceId, Arguments) :-
    member(implementation(Id, Subject,
                          interface_application(InterfaceId, Arguments)), Rows).

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

semantic_application_constructor(Decls, Application) :-
    generic_application_name(Decls, Application),
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
             throw(unsupported_construct(interface_unknown(InterfaceName))) )),
    forall(member_implementation_row(Rows, _, _, InterfaceId, _),
           ( memberchk(declaration(InterfaceId, _, _, interface, _), Rows) -> true
           ; id_kind_name(InterfaceId, interface, InterfaceName),
             throw(unsupported_construct(interface_unknown(InterfaceName))) )).

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
    ),
    ( compile_type_query(Plane, duplicate_implementation(Subject, Interface), _)
    -> throw(unsupported_construct(interface_implementation_duplicate(
                  Subject-Interface)))
    ; true
    ).

% Compile-time relation declarations and source facts.
compile_type_relation(type_plane(_, Rows), interface, [Name, Arity]) :-
    member(declaration(InterfaceId, root, Name, interface, compile_time), Rows),
    findall(_, member(parameter(_, InterfaceId, _, _), Rows), Parameters),
    length(Parameters, Arity).
compile_type_relation(type_plane(_, Rows), implementation,
                      [SubjectName, InterfaceName, ImplId]) :-
    member_implementation_row(Rows, ImplId, Subject, InterfaceId, _),
    id_kind_name(Subject, relation, SubjectName),
    id_kind_name(InterfaceId, interface, InterfaceName).
compile_type_relation(type_plane(_, Rows), implementation,
                      [SubjectName, InterfaceName, Arguments, ImplId]) :-
    member_implementation_row(Rows, ImplId, Subject, InterfaceId, Arguments),
    id_kind_name(Subject, relation, SubjectName),
    id_kind_name(InterfaceId, interface, InterfaceName).
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

% One query boundary for duplicate diagnostics, direct conformance facts, and
% derived conformance rules.  Duplicate checks read authored implementation
% facts so `sort/2` in the normalized catalog cannot hide a repeated source
% declaration.
compile_type_query(type_plane(Decls, _), duplicate_interface(Name), duplicate) :-
    findall(InterfaceName, member(interface_decl(InterfaceName, _), Decls), Names),
    select(Name, Names, Rest),
    memberchk(Name, Rest),
    !.
compile_type_query(type_plane(Decls, _), duplicate_implementation(Subject, Interface),
                   duplicate) :-
    findall(SubjectName-Application,
            ( member(rel_is_implementation(Ref, Applications), Decls),
              ref_name(Ref, SubjectName),
              member(Application, Applications) ),
            Implementations),
    select(Subject-Application, Implementations, Rest),
    memberchk(Subject-Application, Rest),
    Interface = Application,
    !.
compile_type_query(Plane, conforms(Type, interface_pattern(InterfaceName, Patterns)), Proof) :-
    compile_type_conformance(Plane, Type,
                             interface_pattern(InterfaceName, Patterns), [], Proof).
compile_type_query(Plane, conforms(Type, Interface), Proof) :-
    interface_application_parts(Interface, InterfaceName, Patterns),
    compile_type_conformance(Plane, Type, interface_pattern(InterfaceName, Patterns), [], Proof).

% Direct implementation evidence enters the same set evaluator as authored
% compiler facts.  Structural json_encodable evidence remains the existing
% adapter below because its universal field/payload check is not a positive
% row rule in this first slice.
compile_type_conformance(Plane, Type, Interface, _, impl(ImplId)) :-
    atom(Type),
    Interface = interface_pattern(InterfaceName, Patterns),
    Plane = type_plane(Decls, _),
    semantic_type_id(Decls, Type, SubjectId),
    semantic_decl_id(Decls, interface, InterfaceName, InterfaceId),
    interface_pattern_type_ids(Decls, Patterns, PatternIds),
    interface_evidence_closure(Plane, Closure),
    member(interface_evidence(SubjectId, InterfaceId, ConcreteArgs, ImplId),
           Closure),
    interface_arguments_match(PatternIds, ConcreteArgs),
    !.

% A recursive revisit closes the structural proof after direct implementation
% facts, preserving authored `is json_encodable` precedence.
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

interface_evidence_closure(Plane, Closure) :-
    Plane = type_plane(Decls, _),
    findall(interface_evidence(SubjectId, InterfaceId, ArgumentIds, ImplId),
            ( compile_type_relation(Plane, implementation,
                                    [Subject, Interface, Arguments, ImplId]),
              semantic_type_id(Decls, Subject, SubjectId),
              semantic_decl_id(Decls, interface, Interface, InterfaceId),
              maplist(semantic_application_argument_id(Decls), Arguments,
                      ArgumentIds) ),
            SeedRows),
    evaluate_compiler_relations(
        compiler_relations([compiler_relation(interface_evidence/4, 4, [])], []),
        SeedRows, Closure).

interface_pattern_type_ids(Decls, Patterns, PatternIds) :-
    maplist(semantic_application_argument_id(Decls), Patterns, PatternIds).

% Checked at the source terms as well as normalized rows: interface application
% arguments remain visible for arity validation and conformance matching.
validate_interface_applications(Decls) :-
    findall(Name-Arity,
            ( member(interface_decl(Name, Parameters), Decls),
              length(Parameters, Arity) ),
            Interfaces),
    forall(( member(rel_is_implementation(_, Applications), Decls),
             member(Application, Applications) ),
           ( validate_interface_application(Application, Interfaces),
             reject_interface_wildcard(Application) )),
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

reject_interface_wildcard(Application) :-
    interface_application_parts(Application, InterfaceName, Arguments),
    memberchk(any, Arguments),
    throw(unsupported_construct(
              interface_wildcard_outside_bound(InterfaceName))).
reject_interface_wildcard(Application) :-
    interface_application_parts(Application, InterfaceName, Arguments),
    member(Argument, Arguments),
    compound(Argument),
    contains_any(Argument),
    throw(unsupported_construct(
              interface_wildcard_outside_bound(InterfaceName))).
reject_interface_wildcard(_).

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

interface_arguments_match(Patterns, Arguments) :-
    same_length(Patterns, Arguments),
    maplist(interface_argument_matches, Patterns, Arguments).

interface_argument_matches(any, _) :- !.
interface_argument_matches(any_pattern, _) :- !.
interface_argument_matches(Pattern, Argument) :- Pattern == Argument.

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

% The collapsed list column keeps a record of the type it collapsed FROM, the
% option_column/3 precedent: nothing downstream can read `int` and know better.
replace_generic_types(Decls0, Instances, Decls) :-
    maplist(replace_generic_decl(Instances), Decls0, Rewritten),
    findall(list_column(Ref, Column, Type),
            ( member(col_type(Ref, Column, Type), Decls0),
              list_flavor(Type),
              memberchk(Type, Instances) ),
            ListColumns),
    append(Rewritten, ListColumns, Decls).

replace_generic_decl(Instances,
                     col_type(Ref, Column, Type0),
                     col_type(Ref, Column, Type)) :-
    !,
    replace_generic_type(Type0, Instances, Type).
replace_generic_decl(_, Decl, Decl).

%! normalize_key_wrappers(+Decls0, -Decls) is det.
% Normalize key(T) into its value type and the existing keyed/2 declaration.
normalize_key_wrappers(Decls0, Decls) :-
    validate_key_wrapper_types(Decls0),
    findall(Ref-Position,
            key_wrapper_position(Decls0, Ref, _Column, Position),
            WrapperPairs0),
    (   WrapperPairs0 == []
    ->  Decls = Decls0
    ;   sort(WrapperPairs0, WrapperPairs),
        reject_repeated_key_wrapper_positions(WrapperPairs0),
        wrapper_key_positions(WrapperPairs, WrapperKeys),
        reconcile_wrapper_and_legacy_keys(Decls0, WrapperKeys,
                                          CanonicalKeys),
        rewrite_key_declarations(Decls0, CanonicalKeys, Decls)
    ).

% Only an outer column wrapper is accepted; option(T) reaches option expansion.
validate_key_wrapper_types(Decls) :-
    forall(member(col_type(Ref, Column, Type), Decls),
           validate_key_wrapper_type(Decls, Ref, Column, Type)).

validate_key_wrapper_type(Decls, Ref, Column, key(Inner)) :-
    !,
    key_wrapper_column_position(Decls, Ref, Column, Position),
    (   Inner = key(_)
    ->  throw(unsupported_construct(key_wrapper_repeated(Ref, Position)))
    ;   contains_key_wrapper(Inner)
    ->  throw(unsupported_construct(key_wrapper_nested(Ref, Position)))
    ;   true
    ).
validate_key_wrapper_type(Decls, Ref, Column, Type) :-
    (   contains_key_wrapper(Type)
    ->  key_wrapper_column_position(Decls, Ref, Column, Position),
        throw(unsupported_construct(key_wrapper_nested(Ref, Position)))
    ;   true
    ).

contains_key_wrapper(Type) :-
    compound(Type),
    Type =.. [key | _],
    !.
contains_key_wrapper(Type) :-
    compound(Type),
    compound_name_arguments(Type, _, Arguments),
    member(Argument, Arguments),
    contains_key_wrapper(Argument),
    !.

key_wrapper_position(Decls, Ref, Column, Position) :-
    member(col_type(Ref, Column, key(_)), Decls),
    key_wrapper_column_position(Decls, Ref, Column, Position).

key_wrapper_column_position(Decls, Ref, Column, Position) :-
    findall(EachColumn,
            member(col_type(Ref, EachColumn, _), Decls), Columns),
    length(Columns, _Arity),
    nth1(Position, Columns, Column),
    !.

reject_repeated_key_wrapper_positions(Pairs) :-
    member(Ref-Position, Pairs),
    select(Ref-Position, Pairs, Rest),
    memberchk(Ref-Position, Rest),
    !,
    throw(unsupported_construct(key_wrapper_repeated(Ref, Position))).
reject_repeated_key_wrapper_positions(_).

wrapper_key_positions(Pairs, Keys) :-
    findall(Ref, member(Ref-_, Pairs), Refs0),
    sort(Refs0, Refs),
    maplist(wrapper_key_positions_for_ref(Pairs), Refs, Keys).

wrapper_key_positions_for_ref(Pairs, Ref, Ref-Positions) :-
    findall(Position, member(Ref-Position, Pairs), Positions0),
    sort(Positions0, Positions).

% Legacy positions share this canonical relation-column-order representation.
reconcile_wrapper_and_legacy_keys(Decls, WrapperKeys, CanonicalKeys) :-
    maplist(reconcile_one_wrapper_key(Decls), WrapperKeys, CanonicalKeys).

reconcile_one_wrapper_key(Decls, Ref-WrapperPositions,
                          Ref-CanonicalPositions) :-
    findall(Legacy, member(keyed(Ref, Legacy), Decls), LegacyLists),
    (   LegacyLists == []
    ->  CanonicalPositions = WrapperPositions
    ;   maplist(sort, LegacyLists, SortedLegacyLists),
        (   member(LegacyPositions, SortedLegacyLists),
            LegacyPositions \== WrapperPositions
        ->  throw(unsupported_construct(
                      key_wrapper_legacy_conflict(Ref, WrapperPositions,
                                                   LegacyPositions)))
        ;   CanonicalPositions = WrapperPositions
        )
    ).

rewrite_key_declarations(Decls0, CanonicalKeys, Decls) :-
    rewrite_key_declarations(Decls0, CanonicalKeys, [], Decls1, SeenRefs),
    findall(keyed(Ref, Positions),
            ( member(Ref-Positions, CanonicalKeys),
              \+ memberchk(Ref, SeenRefs) ),
            MissingKeys),
    insert_missing_key_declarations(Decls1, MissingKeys, Decls).

insert_missing_key_declarations(Decls, [], Decls) :- !.
insert_missing_key_declarations(Decls0, MissingKeys, Decls) :-
    (   append(Before, [semantic_type_rows(Rows) | After], Decls0)
    ->  append([Before, MissingKeys, [semantic_type_rows(Rows) | After]],
               Decls)
    ;   append(Decls0, MissingKeys, Decls)
    ).

rewrite_key_declarations([], _, Seen, [], Seen).
rewrite_key_declarations([Decl0 | Rest], CanonicalKeys, Seen0,
                         Decls, Seen) :-
    rewrite_one_key_declaration(Decl0, CanonicalKeys, Seen0,
                                Decl, Seen1),
    (   Decl == none
    ->  Decls = RestDecls
    ;   Decls = [Decl | RestDecls]
    ),
    rewrite_key_declarations(Rest, CanonicalKeys, Seen1,
                             RestDecls, Seen).

rewrite_one_key_declaration(col_type(Ref, Column, key(Type)),
                            CanonicalKeys, Seen, col_type(Ref, Column, Type),
                            Seen) :-
    memberchk(Ref-_, CanonicalKeys),
    !.
rewrite_one_key_declaration(keyed(Ref, _), CanonicalKeys, Seen0,
                            Decl, Seen) :-
    memberchk(Ref-Positions, CanonicalKeys),
    !,
    (   memberchk(Ref, Seen0)
    ->  Decl = none,
        Seen = Seen0
    ;   Decl = keyed(Ref, Positions),
        Seen = [Ref | Seen0]
    ).
rewrite_one_key_declaration(Decl, _, Seen, Decl, Seen).

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
% A bare list column stores its entity id and is SPELLED list(Element) to the
% relplan; the named flavors have no boundary render and still collapse.
replace_generic_type(list(Element0), Instances, list(Element)) :-
    memberchk(list(Element0), Instances),
    !,
    replace_generic_type(Element0, Instances, Element).
% Keep annotation syntax attached to the substituted concrete type. Compiler
% relation execution and evidence consumption happen after this phase.
replace_generic_type(annotated_type(Type0, Applications0), Instances,
                     annotated_type(Type, Applications)) :-
    !,
    replace_generic_type(Type0, Instances, Type),
    Applications = Applications0.

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
type_encoding_codes([], [0'l, 0':]) :- !.
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
readable_stem([], Stem) :- !, Stem = 'empty'.
readable_stem(Type, Stem) :-
    is_list(Type), !,
    maplist(readable_stem, Type, Stems),
    atomic_list_concat(Stems, '_', Stem).
readable_stem(Type, Stem) :-
    Type =.. [Constructor | Args],
    maplist(readable_stem, Args, Stems),
    atomic_list_concat([Constructor | Stems], '_', Stem).

generated_generic_name(Name) :-
    atom(Name), sub_atom(Name, 0, _, _, '__gen_').
