:- module(dl7_rust_type_emitter,
          [ render_rust_type_file/4,
            render_rust_type_rows/4
          ]).

:- use_module(library(readutil), [read_file_to_string/3]).
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
% The emitted DL7 is a queryable copy of the accepted Rust type graph. Wire
% ids become file-local generated node names. Source spellings become labels
% only at the `rust_types` product boundary.
render_rust_type_rows(RustPath, Rows, Text, Diagnostics) :-
    accepted_rows(Rows, Accepted),
    read_file_to_string(RustPath, Source, [encoding(utf8)]),
    string_bytes(Source, Bytes, utf8),
    type_ids(Accepted, TypeIds),
    type_names(TypeIds, Accepted, Bytes, Names, NameDiagnostics),
    (   NameDiagnostics == []
    ->  with_output_to(
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

type_names([], _, _, [], []).
type_names([Id | Ids], Rows, Bytes,
           [type_name(Id, Name, Start) | Names], Diagnostics) :-
    type_name_result(Rows, Bytes, Id, Result),
    type_names(Ids, Rows, Bytes, Names0, RestDiagnostics),
    combine_type_name(Result, Id, Names0, RestDiagnostics,
                      Name, Start, Names, Diagnostics).

combine_type_name(ok(Name, Start), _, Names, Diagnostics,
                  Name, Start, Names, Diagnostics).
combine_type_name(error(Detail), Id, Names, Diagnostics,
                  Generated, 0, Names,
                  [diagnostic(emit, none,
                              rust_type_name(Id, Detail)) | Diagnostics]) :-
    generated_type_name(Id, Generated).

type_name_result(Rows, Bytes, Id, Result) :-
    findall(Start-End,
            member(extract_fact(_, 'tsi.origin',
                                [id(Id), atom(rust), span(_, Start, End)]),
                   Rows),
            Spans0),
    sort(Spans0, Spans),
    (   Spans = [Start-End | _],
        span_text(Bytes, Start, End, Written),
        identifier_text(Written, Name)
    ->  Result = ok(Name, Start)
    ;   Result = error(missing_or_invalid_origin)
    ).

span_text(Bytes, Start, End, Text) :-
    integer(Start),
    integer(End),
    Start >= 0,
    End >= Start,
    length(Prefix, Start),
    append(Prefix, Rest, Bytes),
    Length is End - Start,
    length(Slice, Length),
    append(Slice, _, Rest),
    string_bytes(Text, Slice, utf8).

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

generated_type_name(Id, Name) :-
    format(atom(Name), 'rust_type_~d', [Id]).

write_type_graph(RustPath, Rows, TypeIds, Names) :-
    format('; generated from ~w~n', [RustPath]),
    write_public_type_product(Rows, Names),
    nl,
    write_type_nodes(TypeIds, Rows),
    write_metadata(Rows).

write_public_type_product(Rows, Names) :-
    root_type_names(Rows, Names, Roots),
    writeln('(: rust_types'),
    write_edges_form('*', Roots),
    writeln(')').

root_type_names(Rows, Names, Roots) :-
    findall(Start-Name-Id,
            ( member(type_name(Id, Name, Start), Names),
              root_type(Rows, Id)
            ),
            Roots0),
    sort(Roots0, Ordered),
    unique_root_labels(Ordered, Roots).

root_type(Rows, Id) :- memberchk(extract_fact(_, 'tsi.product', [id(Id)]), Rows), !.
root_type(Rows, Id) :- memberchk(extract_fact(_, 'tsi.sum', [id(Id)]), Rows), !.
root_type(Rows, Id) :- memberchk(extract_fact(_, 'rust.trait', [id(Id)]), Rows).

unique_root_labels(Roots, Edges) :-
    unique_root_labels(Roots, [], Edges).

unique_root_labels([], _, []).
unique_root_labels([_-Name-Id | Roots], Seen,
                   [edge(Label, Target) | Edges]) :-
    unique_label(Name, Id, Seen, Label),
    generated_type_name(Id, Target),
    unique_root_labels(Roots, [Label | Seen], Edges).

unique_label(Name, _, Seen, Name) :- \+ memberchk(Name, Seen), !.
unique_label(Name, Id, _, Label) :- format(atom(Label), '~w_~d', [Name, Id]).

write_type_nodes([], _).
write_type_nodes([Id | Ids], Rows) :-
    generated_type_name(Id, TypeName),
    type_operator(Rows, Id, Operator),
    type_edges(Rows, Id, Edges),
    format('(: ~w~n', [TypeName]),
    write_edges_form(Operator, Edges),
    writeln(')'),
    nl,
    write_type_nodes(Ids, Rows).

type_operator(Rows, Id, '+') :-
    memberchk(extract_fact(_, 'tsi.sum', [id(Id)]), Rows),
    !.
type_operator(_, _, '*').

type_edges(Rows, Owner, Edges) :-
    findall(Position-LabelText-Target,
            member(extract_fact(_, 'tsi.edge',
                                [id(_), id(Owner), text(LabelText),
                                 id(Target), int(Position)]),
                   Rows),
            Edges0),
    sort(Edges0, Ordered),
    maplist(type_edge, Ordered, Edges).

type_edge(_-LabelText-Target, edge(Label, TargetName)) :-
    identifier_text(LabelText, Label),
    generated_type_name(Target, TargetName).

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

write_metadata(Rows) :-
    write_trait_metadata(Rows),
    write_callable_metadata(Rows),
    write_callable_slot_metadata(Rows, 'tsi.input', tsi_input),
    write_callable_slot_metadata(Rows, 'tsi.output', tsi_output),
    write_parameter_metadata(Rows),
    write_impl_metadata(Rows),
    write_assoc_metadata(Rows),
    write_conformance_metadata(Rows).

write_trait_metadata(Rows) :-
    findall(Id, member(extract_fact(_, 'rust.trait', [id(Id)]), Rows), Ids0),
    sort(Ids0, Ids),
    write_unary_metadata(rust_trait, Ids).

write_unary_metadata(_, []).
write_unary_metadata(Relation, Ids) :-
    Ids = [_ | _],
    format('(: ~w (* (: node type)))~n', [Relation]),
    forall(member(Id, Ids),
           ( generated_type_name(Id, Name),
             format('(~w ~w)~n', [Relation, Name])
           )),
    nl.

write_callable_metadata(Rows) :-
    findall(Id,
            member(extract_fact(_, 'tsi.callable', [id(Id)]), Rows),
            Ids0),
    sort(Ids0, Ids),
    write_unary_metadata(tsi_callable, Ids).

write_callable_slot_metadata(Rows, WireRelation, OutputRelation) :-
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
               write_callable_slot(OutputRelation, Callable,
                                   Position, Target)),
        nl
    ).

write_callable_slot(Relation, Callable, Position, Target) :-
    generated_type_name(Callable, CallableName),
    generated_type_name(Target, TargetName),
    format('(~w ~w ~d ~w)~n',
           [Relation, CallableName, Position, TargetName]).

write_parameter_metadata(Rows) :-
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
               write_parameter(Owner, Position, Parameter, Variance)),
        nl
    ).

write_parameter(Owner, Position, Parameter, Variance) :-
    generated_type_name(Owner, OwnerName),
    generated_type_name(Parameter, ParameterName),
    format('(tsi_parameter ~w ~w ~d "~w")~n',
           [ParameterName, OwnerName, Position, Variance]).

write_impl_metadata(Rows) :-
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
               write_impl(Impl, Self, Trait)),
        nl
    ).

write_impl(Impl, Self, Trait) :-
    generated_type_name(Impl, ImplName),
    generated_type_name(Self, SelfName),
    generated_type_name(Trait, TraitName),
    format('(rust_impl ~w ~w ~w)~n', [ImplName, SelfName, TraitName]).

write_assoc_metadata(Rows) :-
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
               write_assoc(Owner, Name, Target)),
        nl
    ).

write_assoc(Owner, Name, Target) :-
    generated_type_name(Owner, OwnerName),
    generated_type_name(Target, TargetName),
    format('(rust_assoc ~w "~s" ~w)~n', [OwnerName, Name, TargetName]).

write_conformance_metadata(Rows) :-
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
               write_conformance(Source, Target, Mode))
    ).

write_conformance(Source, Target, Mode) :-
    generated_type_name(Source, SourceName),
    generated_type_name(Target, TargetName),
    format('(tsi_conforms ~w ~w "~w")~n',
           [SourceName, TargetName, Mode]).
