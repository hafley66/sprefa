:- module(dl7_source_fact_loader,
          [ load_source_fact_files/3,
            install_source_fact_graph/6
          ]).

:- use_module(library(error), [must_be/2]).
:- use_module(library(http/json), [atom_json_dict/3]).
:- use_module(library(readutil), [read_file_to_string/3]).

% Normalized source observations arrive as the compact envelope emitted by
% sprefa-extract. This loader expands range-local data into shared content-span
% and located-occurrence identities before those rows enter the DL7 fixpoint.

source_relation(source, 1).
source_relation(source_directory, 3).
source_relation(source_git, 4).
source_relation(source_revision, 1).
source_relation(source_content, 2).
source_relation(content, 2).
source_relation(git_blob, 3).
source_relation(parse, 6).
source_relation(source_query, 3).
source_relation(content_span, 4).
source_relation(located, 3).
source_relation(source_match, 4).
source_relation(source_capture, 4).
source_relation(source_replacement, 4).

%% load_source_fact_files(+Paths, -Rows, -Diagnostics) is det.
%
% Every file contains one JSON array of protocol-1 source-query envelopes.
% Equal logical rows from several query engines collapse before installation.
load_source_fact_files(Paths, Rows, Diagnostics) :-
    must_be(list, Paths),
    load_source_fact_files_(Paths, Rows0, Diagnostics0),
    sort(Rows0, Rows),
    sort(Diagnostics0, Diagnostics).

load_source_fact_files_([], [], []).
load_source_fact_files_([Path | Paths], Rows, Diagnostics) :-
    load_source_fact_file(Path, FileRows, FileDiagnostics),
    load_source_fact_files_(Paths, RestRows, RestDiagnostics),
    append(FileRows, RestRows, Rows),
    append(FileDiagnostics, RestDiagnostics, Diagnostics).

load_source_fact_file(Path, Rows, Diagnostics) :-
    must_be(ground, Path),
    catch(read_source_fact_file(Path, Rows, Diagnostics),
          Error,
          ( Rows = [],
            Diagnostics = [diagnostic(source, file(Path),
                                      source_fact_read_error(Error))]
          )).

read_source_fact_file(Path, Rows, Diagnostics) :-
    read_file_to_string(Path, Text, [encoding(utf8)]),
    atom_json_dict(Text, Envelopes, [value_string_as(string)]),
    (   is_list(Envelopes)
    ->  decode_envelopes(Envelopes, Path, 0, Rows, Diagnostics)
    ;   Rows = [],
        Diagnostics = [diagnostic(source, file(Path),
                                  source_fact_root_not_array)]
    ).

decode_envelopes([], _, _, [], []).
decode_envelopes([Envelope | Envelopes], Path, Position, Rows, Diagnostics) :-
    decode_envelope_result(Envelope, Path, Position, Result),
    NextPosition is Position + 1,
    decode_envelopes(Envelopes, Path, NextPosition,
                     RestRows, RestDiagnostics),
    combine_envelope_result(Result, RestRows, RestDiagnostics,
                            Rows, Diagnostics).

decode_envelope_result(Envelope, Path, Position, Result) :-
    (   catch(decode_envelope(Envelope, Rows), _, fail)
    ->  Result = rows(Rows)
    ;   Result = error(diagnostic(source, file(Path),
                                  malformed_source_envelope(Position)))
    ).

combine_envelope_result(rows(FileRows), RestRows, Diagnostics,
                        Rows, Diagnostics) :-
    append(FileRows, RestRows, Rows).
combine_envelope_result(error(Diagnostic), Rows, Diagnostics,
                        Rows, [Diagnostic | Diagnostics]).

decode_envelope(Envelope, Rows) :-
    is_dict(Envelope),
    get_dict(protocol, Envelope, 1),
    get_dict(source, Envelope, SourceDict),
    source_rows(SourceDict, Source, SourceRows),
    get_dict(content, Envelope, ContentText),
    string(ContentText),
    atom_string(ContentDigest, ContentText),
    Content = content(ContentDigest),
    get_dict(byte_length, Envelope, ByteLength),
    integer(ByteLength),
    get_dict(git_blobs, Envelope, GitBlobs),
    git_blob_rows(GitBlobs, Content, GitRows),
    get_dict(parse, Envelope, ParseDict),
    parse_row(ParseDict, Content, Parse, ParseRow),
    get_dict(query, Envelope, QueryDict),
    query_row(QueryDict, Query, QueryRow),
    get_dict(matches, Envelope, Matches),
    match_rows(Matches, Source, Content, Parse, Query, MatchRows),
    append([ SourceRows,
             [ source_row(source_content, [ref(Source), ref(Content)]),
               source_row(content, [ref(Content), const(ByteLength)]),
               ParseRow,
               QueryRow
             ],
             GitRows,
             MatchRows
           ],
           Rows).

source_rows(Dict, Source, Rows) :-
    dict_atom(Dict, kind, directory),
    !,
    dict_string(Dict, directory, Directory),
    dict_string(Dict, path, Path),
    Source = directory_source(Directory, Path),
    Rows = [source_row(source, [ref(Source)]),
            source_row(source_directory,
                       [ref(Source), const(Directory), const(Path)])].
source_rows(Dict, Source, Rows) :-
    dict_atom(Dict, kind, git),
    dict_string(Dict, repository, Repository),
    dict_string(Dict, path, Path),
    get_dict(revision, Dict, RevisionDict),
    source_revision(RevisionDict, Revision),
    Source = git_source(Repository, Revision, Path),
    Rows = [source_row(source, [ref(Source)]),
            source_row(source_revision, [ref(Revision)]),
            source_row(source_git,
                       [ref(Source), const(Repository), ref(Revision),
                        const(Path)])].

source_revision(Dict, commit(Object)) :-
    dict_atom(Dict, kind, commit),
    dict_string(Dict, object, Object).
source_revision(Dict, worktree(Worktree, Head, Dirty)) :-
    dict_atom(Dict, kind, worktree),
    dict_string(Dict, worktree, Worktree),
    nullable_string(Dict, head, Head),
    get_dict(dirty, Dict, Dirty),
    memberchk(Dirty, [true, false]).

git_blob_rows([], _, []).
git_blob_rows([Dict | Dicts], Content,
              [source_row(git_blob,
                          [ref(Content), const(Repository), const(Object)])
              | Rows]) :-
    dict_string(Dict, repository, Repository),
    dict_string(Dict, object, Object),
    git_blob_rows(Dicts, Content, Rows).

parse_row(Dict, Content, Parse,
          source_row(parse,
                     [ ref(Parse), ref(Content), const(Grammar),
                       const(Engine), const(Version), const(Configuration)
                     ])) :-
    dict_atom(Dict, grammar, Grammar),
    dict_atom(Dict, engine, Engine),
    dict_string(Dict, version, Version),
    get_dict(configuration, Dict, ConfigurationJson),
    json_term(ConfigurationJson, Configuration),
    Parse = parse(Content, Grammar, Engine, Version, Configuration).

query_row(Dict, Query,
          source_row(source_query,
                     [ref(Query), const(Engine), const(Specification)])) :-
    dict_atom(Dict, engine, Engine),
    get_dict(specification, Dict, SpecificationJson),
    json_term(SpecificationJson, Specification),
    Query = source_query(Engine, Specification).

match_rows([], _, _, _, _, []).
match_rows([Dict | Dicts], Source, Content, Parse, Query, Rows) :-
    get_dict(position, Dict, Position),
    integer(Position),
    dict_atom(Dict, branch, Branch),
    get_dict(pattern, Dict, Pattern),
    integer(Pattern),
    get_dict(range, Dict, Range),
    range_identity(Range, Content, Span, SpanRows),
    Match = source_match(Query, Parse, Position, Branch, Pattern, Span),
    MatchRow = source_row(source_match,
                          [ref(Match), ref(Query), ref(Parse), ref(Span)]),
    get_dict(captures, Dict, Captures),
    capture_rows(Captures, Match, Source, Content, CaptureRows),
    get_dict(replacement, Dict, Replacement),
    replacement_rows(Replacement, Match, Source, Span, ReplacementRows),
    match_rows(Dicts, Source, Content, Parse, Query, RestRows),
    append([SpanRows, [MatchRow], CaptureRows, ReplacementRows, RestRows],
           Rows).

capture_rows([], _, _, _, []).
capture_rows([Dict | Dicts], Match, Source, Content, Rows) :-
    get_dict(position, Dict, Position),
    integer(Position),
    dict_atom(Dict, label, Label),
    get_dict(range, Dict, Range),
    range_identity(Range, Content, Span, SpanRows),
    CaptureRow = source_row(source_capture,
                            [ ref(Match), const(Position), const(Label),
                              ref(Span)
                            ]),
    located_rows(Source, Span, LocatedRows),
    capture_rows(Dicts, Match, Source, Content, RestRows),
    append([SpanRows, LocatedRows, [CaptureRow], RestRows], Rows).

replacement_rows(null, _, _, _, []) :- !.
replacement_rows(@(null), _, _, _, []) :- !.
replacement_rows(Dict, Match, Source, Span,
                 [ source_row(located,
                              [ref(Occurrence), ref(Source), ref(Span)]),
                   source_row(source_replacement,
                              [ ref(Edit), ref(Occurrence),
                                const(Replacement), const(Producer)
                              ])
                 ]) :-
    dict_string(Dict, replacement, Replacement),
    dict_atom(Dict, producer, Producer),
    Occurrence = located(Source, Span),
    Edit = source_replacement(Match, Occurrence, Producer).

range_identity(Dict, Content, Span,
               [source_row(content_span,
                           [ ref(Span), ref(Content), const(Start), const(End)
                           ])]) :-
    get_dict(start, Dict, Start),
    get_dict(end, Dict, End),
    integer(Start),
    integer(End),
    Start =< End,
    Span = content_span(Content, Start, End).

located_rows(Source, Span,
             [source_row(located,
                         [ref(Occurrence), ref(Source), ref(Span)])]) :-
    Occurrence = located(Source, Span).

dict_atom(Dict, Key, Atom) :-
    dict_string(Dict, Key, String),
    atom_string(Atom, String).

dict_string(Dict, Key, String) :-
    get_dict(Key, Dict, String),
    string(String).

nullable_string(Dict, Key, none) :-
    get_dict(Key, Dict, Null),
    memberchk(Null, [null, @(null)]),
    !.
nullable_string(Dict, Key, some(String)) :-
    dict_string(Dict, Key, String).

% JSON values become sorted, typed Prolog terms so object-key order and scalar
% representation cannot change parse or query identity.
json_term(Value, text(Value)) :- string(Value), !.
json_term(Value, integer(Value)) :- integer(Value), !.
json_term(Value, number(Value)) :- number(Value), !.
json_term(true, boolean(true)) :- !.
json_term(false, boolean(false)) :- !.
json_term(@(true), boolean(true)) :- !.
json_term(@(false), boolean(false)) :- !.
json_term(null, null) :- !.
json_term(@(null), null) :- !.
json_term(Value, array(Items)) :-
    is_list(Value),
    !,
    maplist(json_term, Value, Items).
json_term(Value, object(Pairs)) :-
    is_dict(Value),
    dict_pairs(Value, _, Pairs0),
    maplist(json_pair, Pairs0, Pairs1),
    sort(Pairs1, Pairs).

json_pair(Key-Value, Key-Normalized) :-
    json_term(Value, Normalized).

%% install_source_fact_graph(+Rows, +Basements0, +Origins0,
%%                           -Basements, -Origins, -Diagnostics) is det.
install_source_fact_graph(Rows, Basements0, Origins0,
                          [module_basement(Owner, Basement) | Basements0],
                          [module_origins(Owner, []) | Origins0], []) :-
    must_be(ground, Rows),
    Owner = module(source_intelligence),
    source_relations(Relations, Names),
    source_seeds(Owner, Rows, Seeds),
    relation_edges(Owner, Names, 0, Edges),
    Basement = basement_program(
                   root_graph([node(Owner), module(Owner), product(Owner)],
                              Edges),
                   datalog_program(Relations, Seeds, [])).

source_relations(Relations, Names) :-
    findall(Name-Arity, source_relation(Name, Arity), Pairs),
    pairs_relations(Pairs, Names, Relations).

pairs_relations([], [], []).
pairs_relations([Name-Arity | Pairs], [Name | Names],
                [relation(source_relation(Name), Arity, []) | Relations]) :-
    pairs_relations(Pairs, Names, Relations).

source_seeds(_, [], []).
source_seeds(Owner, [source_row(Name, Arguments) | Rows],
             [call(name(Owner, Name), Arguments) | Seeds]) :-
    source_seeds(Owner, Rows, Seeds).

relation_edges(_, [], _, []).
relation_edges(Owner, [Name | Names], Index,
               [pending_edge(Owner, Name, target(source_relation(Name)), Index)
               | Edges]) :-
    NextIndex is Index + 1,
    relation_edges(Owner, Names, NextIndex, Edges).
