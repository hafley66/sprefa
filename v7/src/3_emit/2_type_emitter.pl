:- module(dl7_rust_type_emitter,
          [ render_rust_type_file/4,
            render_rust_type_rows/4
          ]).

:- use_module('../2_comptime/0c_extract_loader',
              [load_tsi_stream/3, accepted_rows/2]).

%% render_rust_type_file(+RustPath, +TsiPath, -Text, -Diagnostics) is det.
render_rust_type_file(RustPath, TsiPath, Text, Diagnostics) :-
    load_tsi_stream(TsiPath, Rows, LoadDiagnostics),
    (   LoadDiagnostics == []
    ->  render_rust_type_rows(RustPath, Rows, Text, Diagnostics)
    ;   Text = "",
        Diagnostics = LoadDiagnostics
    ).

%% render_rust_type_rows(+RustPath, +TsiRows, -Text, -Diagnostics) is det.
%
% The emitted DL7 is a queryable copy of the accepted Rust type graph. Every
% named source type becomes a direct DL7 binding. A repeated spelling receives
% its wire id as a suffix until scoped TSI names are available.
render_rust_type_rows(RustPath, Rows, Text, Diagnostics) :-
    accepted_rows(Rows, Accepted),
    type_ids(Accepted, TypeIds),
    type_names(TypeIds, Accepted, SourceNames, NameDiagnostics),
    (   NameDiagnostics == []
    ->  unique_type_names(Accepted, SourceNames, Names),
        with_output_to(
            string(Text),
            write_type_graph(RustPath, Accepted, TypeIds, Names)),
        Diagnostics = []
    ;   Text = "",
        Diagnostics = NameDiagnostics
    ).

type_ids(Rows, Ids) :-
    findall(Id,
            member(extract_fact(_, 'tsi.type', [id(Id)]), Rows),
            Ids0),
    sort(Ids0, Ids).

type_names([], _, [], []).
type_names([Id | Ids], Rows,
           [type_name(Id, Name, Start) | Names], Diagnostics) :-
    type_name_result(Rows, Id, Result),
    type_names(Ids, Rows, Names0, RestDiagnostics),
    combine_type_name(Result, Id, Names0, RestDiagnostics,
                      Name, Start, Names, Diagnostics).

combine_type_name(ok(Name, Start), _, Names, Diagnostics,
                  Name, Start, Names, Diagnostics).
combine_type_name(error(Detail), Id, Names, Diagnostics,
                  invalid, 0, Names,
                  [diagnostic(emit, none,
                              type_name(Id, Detail)) | Diagnostics]).

type_name_result(Rows, Id, Result) :-
    findall(Written,
            member(extract_fact(_, 'tsi.name', [id(Id), text(Written)]),
                   Rows),
            Written0),
    sort(Written0, WrittenNames),
    (   WrittenNames = [Written]
    ->  source_name_identifier(Written, NameResult),
        name_result(NameResult, Id, Result)
    ;   derived_type_name(Rows, Id, Name)
    ->  Result = ok(Name, Id)
    ;   Result = error(missing_or_invalid_name)
    ).

source_name_identifier("()", ok(unit)) :- !.
source_name_identifier(Written, ok(Name)) :-
    identifier_text(Written, Name),
    !.
source_name_identifier(Written, error(invalid_name(Written))).

name_result(ok(Name), Start, ok(Name, Start)).
name_result(error(Detail), _, error(Detail)).

derived_type_name(Rows, Id, Name) :-
    member(extract_fact(_, 'rust.impl',
                        [id(Id), id(Self), id(Trait)]), Rows),
    wire_name(Rows, Self, SelfName),
    wire_name(Rows, Trait, TraitName),
    !,
    format(atom(Name), '~w_~w_impl', [SelfName, TraitName]).
derived_type_name(Rows, Id, Name) :-
    member(extract_fact(_, 'tsi.edge',
                        [id(_), id(Owner), text(Label), id(Id), int(_)]),
           Rows),
    wire_name(Rows, Owner, OwnerName),
    identifier_text(Label, LabelName),
    !,
    format(atom(Name), '~w_~w', [OwnerName, LabelName]).
derived_type_name(Rows, Id, Name) :-
    memberchk(extract_fact(_, 'tsi.product', [id(Id)]), Rows),
    format(atom(Name), 'anonymous_product_~d', [Id]).

wire_name(Rows, Id, Name) :-
    member(extract_fact(_, 'tsi.name', [id(Id), text(Written)]), Rows),
    source_name_identifier(Written, ok(Name)),
    !.

identifier_text(Text, Identifier) :-
    string_codes(Text, Codes),
    maplist(identifier_code, Codes, SafeCodes0),
    collapse_underscores(SafeCodes0, SafeCodes1),
    trim_underscores(SafeCodes1, SafeCodes),
    SafeCodes = [First | _],
    (   code_type(First, digit)
    ->  atom_codes(Identifier, [0'_|SafeCodes])
    ;   atom_codes(Identifier, SafeCodes)
    ).

identifier_code(Code, Code) :-
    (code_type(Code, alnum); Code =:= 0'_),
    !.
identifier_code(_, 0'_).

collapse_underscores([], []).
collapse_underscores([0'_, 0'_ | Codes], Collapsed) :-
    !,
    collapse_underscores([0'_ | Codes], Collapsed).
collapse_underscores([Code | Codes], [Code | Collapsed]) :-
    collapse_underscores(Codes, Collapsed).

trim_underscores(Codes, Trimmed) :-
    drop_leading_underscores(Codes, FrontTrimmed),
    reverse(FrontTrimmed, Reversed),
    drop_leading_underscores(Reversed, BackTrimmed),
    reverse(BackTrimmed, Trimmed).

drop_leading_underscores([0'_ | Codes], Trimmed) :-
    !,
    drop_leading_underscores(Codes, Trimmed).
drop_leading_underscores(Codes, Codes).

unique_type_names(Rows, SourceNames, Names) :-
    findall(Start-Id-SourceName-Candidate,
            ( member(type_name(Id, SourceName, Start), SourceNames),
              candidate_type_name(Rows, SourceNames, Id, SourceName,
                                  Candidate)
            ),
            Ordered0),
    sort(Ordered0, Ordered),
    allocate_type_names(Ordered, [], Names0),
    sort(Names0, Names).

candidate_type_name(Rows, SourceNames, Id, SourceName, Candidate) :-
    member(extract_fact(_, 'tsi.parameter',
                        [id(Id), id(Owner), int(_), atom(_)]), Rows),
    memberchk(type_name(Owner, OwnerName, _), SourceNames),
    !,
    format(atom(Candidate), '~w_~w', [OwnerName, SourceName]).
candidate_type_name(Rows, SourceNames, Id, SourceName, Candidate) :-
    member(extract_fact(_, 'tsi.edge',
                        [id(_), id(Owner), text(Label), id(Id), int(_)]),
           Rows),
    atom_string(SourceName, Label),
    memberchk(type_name(Owner, OwnerName, _), SourceNames),
    !,
    format(atom(Candidate), '~w_~w', [OwnerName, SourceName]).
candidate_type_name(_, _, _, SourceName, SourceName).

allocate_type_names([], _, []).
allocate_type_names([Start-Id-SourceName-Candidate | Rest], Used,
                    [type_name(Id, SourceName, Start, Name) | Names]) :-
    unique_type_name(Candidate, Id, Used, Name),
    allocate_type_names(Rest, [Name | Used], Names).

unique_type_name(SourceName, _, Used, SourceName) :-
    \+ memberchk(SourceName, Used),
    !.
unique_type_name(SourceName, Id, Used, Name) :-
    format(atom(Candidate), '~w_~d', [SourceName, Id]),
    unique_generated_name(Candidate, 2, Used, Name).

unique_generated_name(Candidate, _, Used, Candidate) :-
    \+ memberchk(Candidate, Used),
    !.
unique_generated_name(Candidate, Suffix, Used, Name) :-
    format(atom(Next), '~w_~d', [Candidate, Suffix]),
    NextSuffix is Suffix + 1,
    unique_generated_name(Next, NextSuffix, Used, Name).

emitted_type_name(Names, Id, Name) :-
    memberchk(type_name(Id, _, _, Name), Names).

write_type_graph(RustPath, Rows, TypeIds, Names) :-
    format('; generated from ~w~n', [RustPath]),
    write_type_nodes(TypeIds, Rows, Names),
    write_metadata(Rows, Names).

write_type_nodes([], _, _).
write_type_nodes([Id | Ids], Rows, Names) :-
    emitted_type_name(Names, Id, TypeName),
    type_operator(Rows, Id, Operator),
    type_edges(Rows, Names, Id, Edges),
    format('(: ~w~n', [TypeName]),
    write_edges_form(Operator, Edges),
    writeln(')'),
    nl,
    write_type_nodes(Ids, Rows, Names).

type_operator(Rows, Id, '+') :-
    memberchk(extract_fact(_, 'tsi.sum', [id(Id)]), Rows),
    !.
type_operator(_, _, '*').

type_edges(Rows, Names, Owner, Edges) :-
    findall(Position-LabelText-Target,
            member(extract_fact(_, 'tsi.edge',
                                [id(_), id(Owner), text(LabelText),
                                 id(Target), int(Position)]),
                   Rows),
            Edges0),
    sort(Edges0, Ordered),
    maplist(type_edge(Names), Ordered, Edges).

type_edge(Names, _-LabelText-Target, edge(Label, TargetName)) :-
    identifier_text(LabelText, Label),
    emitted_type_name(Names, Target, TargetName).

write_edges_form(Operator, []) :-
    format('   (~w)', [Operator]).
write_edges_form(Operator, [edge(Label, Target) | Edges]) :-
    format('   (~w (: ~w ~w)', [Operator, Label, Target]),
    write_following_edges(Edges),
    write(')').

write_following_edges([]).
write_following_edges([edge(Label, Target) | Edges]) :-
    format('~n      (: ~w ~w)', [Label, Target]),
    write_following_edges(Edges).

write_metadata(Rows, Names) :-
    write_trait_metadata(Rows, Names),
    write_callable_metadata(Rows, Names),
    write_callable_slot_metadata(Rows, Names, 'tsi.input', tsi_input),
    write_callable_slot_metadata(Rows, Names, 'tsi.output', tsi_output),
    write_parameter_metadata(Rows, Names),
    write_impl_metadata(Rows, Names),
    write_assoc_metadata(Rows, Names),
    write_conformance_metadata(Rows, Names).

write_trait_metadata(Rows, Names) :-
    findall(Id, member(extract_fact(_, 'rust.trait', [id(Id)]), Rows), Ids0),
    sort(Ids0, Ids),
    write_unary_metadata(rust_trait, Ids, Names).

write_unary_metadata(_, [], _).
write_unary_metadata(Relation, Ids, Names) :-
    Ids = [_ | _],
    format('(: ~w (* (: node type)))~n', [Relation]),
    forall(member(Id, Ids),
           ( emitted_type_name(Names, Id, Name),
             format('(~w ~w)~n', [Relation, Name])
           )),
    nl.

write_callable_metadata(Rows, Names) :-
    findall(Id,
            member(extract_fact(_, 'tsi.callable', [id(Id)]), Rows),
            Ids0),
    sort(Ids0, Ids),
    write_unary_metadata(tsi_callable, Ids, Names).

write_callable_slot_metadata(Rows, Names, WireRelation, OutputRelation) :-
    findall(Callable-Position-Target,
            member(extract_fact(_, WireRelation,
                                [id(Callable), int(Position), id(Target)]),
                   Rows),
            Slots0),
    sort(Slots0, Slots),
    (   Slots == []
    ->  true
    ;   format('(: ~w~n', [OutputRelation]),
        writeln('   (* (: callable type)'),
        writeln('      (: position int)'),
        writeln('      (: target type)))'),
        forall(member(Callable-Position-Target, Slots),
               write_callable_slot(OutputRelation, Names, Callable,
                                   Position, Target)),
        nl
    ).

write_callable_slot(Relation, Names, Callable, Position, Target) :-
    emitted_type_name(Names, Callable, CallableName),
    emitted_type_name(Names, Target, TargetName),
    format('(~w ~w ~d ~w)~n',
           [Relation, CallableName, Position, TargetName]).

write_parameter_metadata(Rows, Names) :-
    findall(Owner-Position-Parameter-Variance,
            member(extract_fact(_, 'tsi.parameter',
                                [id(Parameter), id(Owner), int(Position),
                                 atom(Variance)]),
                   Rows),
            Parameters0),
    sort(Parameters0, Parameters),
    (   Parameters == []
    ->  true
    ;   writeln('(: tsi_parameter'),
        writeln('   (* (: parameter type)'),
        writeln('      (: owner type)'),
        writeln('      (: position int)'),
        writeln('      (: variance text)))'),
        forall(member(Owner-Position-Parameter-Variance, Parameters),
               write_parameter(Names, Owner, Position, Parameter, Variance)),
        nl
    ).

write_parameter(Names, Owner, Position, Parameter, Variance) :-
    emitted_type_name(Names, Owner, OwnerName),
    emitted_type_name(Names, Parameter, ParameterName),
    format('(tsi_parameter ~w ~w ~d "~w")~n',
           [ParameterName, OwnerName, Position, Variance]).

write_impl_metadata(Rows, Names) :-
    findall(Impl-Self-Trait,
            member(extract_fact(_, 'rust.impl',
                                [id(Impl), id(Self), id(Trait)]), Rows),
            Impls0),
    sort(Impls0, Impls),
    (   Impls == []
    ->  true
    ;   writeln('(: rust_impl'),
        writeln('   (* (: implementation type)'),
        writeln('      (: self type)'),
        writeln('      (: trait type)))'),
        forall(member(Impl-Self-Trait, Impls),
               write_impl(Names, Impl, Self, Trait)),
        nl
    ).

write_impl(Names, Impl, Self, Trait) :-
    emitted_type_name(Names, Impl, ImplName),
    emitted_type_name(Names, Self, SelfName),
    emitted_type_name(Names, Trait, TraitName),
    format('(rust_impl ~w ~w ~w)~n', [ImplName, SelfName, TraitName]).

write_assoc_metadata(Rows, Names) :-
    findall(Owner-Name-Target,
            member(extract_fact(_, 'rust.assoc',
                                [id(Owner), text(Name), id(Target)]), Rows),
            Assocs0),
    sort(Assocs0, Assocs),
    (   Assocs == []
    ->  true
    ;   writeln('(: rust_assoc'),
        writeln('   (* (: owner type)'),
        writeln('      (: name text)'),
        writeln('      (: target type)))'),
        forall(member(Owner-Name-Target, Assocs),
               write_assoc(Names, Owner, Name, Target)),
        nl
    ).

write_assoc(Names, Owner, Name, Target) :-
    emitted_type_name(Names, Owner, OwnerName),
    emitted_type_name(Names, Target, TargetName),
    format('(rust_assoc ~w "~s" ~w)~n', [OwnerName, Name, TargetName]).

write_conformance_metadata(Rows, Names) :-
    findall(Source-Target-Mode,
            member(extract_fact(_, 'tsi.conforms',
                                [id(Source), id(Target), atom(Mode)]), Rows),
            Conforms0),
    sort(Conforms0, Conforms),
    (   Conforms == []
    ->  true
    ;   writeln('(: tsi_conforms'),
        writeln('   (* (: source type)'),
        writeln('      (: target type)'),
        writeln('      (: mode text)))'),
        forall(member(Source-Target-Mode, Conforms),
               write_conformance(Names, Source, Target, Mode))
    ).

write_conformance(Names, Source, Target, Mode) :-
    emitted_type_name(Names, Source, SourceName),
    emitted_type_name(Names, Target, TargetName),
    format('(tsi_conforms ~w ~w "~w")~n',
           [SourceName, TargetName, Mode]).
