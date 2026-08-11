:- module(parse_parity,
          [ run/0,
            compare_files/3,
            corpus_files/1,
            worker_main/0
          ]).

:- use_module(library(filesex)).
:- use_module(library(lists)).
:- use_module(library(process)).

:- dynamic(script_file/1).
:- prolog_load_context(file, Script), assertz(script_file(Script)).

run :-
    corpus_files(Files),
    compare_files(Files, Mode, summary(Parity, Skips, Diffs)),
    length(Files, Total),
    format('PARSE_PARITY mode=~w total=~d parity=~d skips=~d diffs=~d~n',
           [Mode, Total, Parity, Skips, Diffs]),
    Diffs =:= 0.

corpus_files(Files) :-
    v6_dir(V6),
    maplist(path_from(V6),
            [ 'prolog/conformance/fixtures',
              'prolog/compile/out/text-door',
              'prolog/compile/dl_view',
              'dl/fixtures'
            ],
            Roots),
    findall(File,
            ( member(Root, Roots),
              exists_directory(Root),
              directory_member(Root, File,
                               [ recursive(true), extensions([dl6]),
                                 file_type(regular) ])
            ),
            RootFiles),
    path_from(V6, 'tsv2/gen/pokeapi_gen.dl6', PokeApi),
    ( exists_file(PokeApi) -> Extra = [PokeApi] ; Extra = [] ),
    append(RootFiles, Extra, Unsorted),
    sort(Unsorted, Files).

compare_files(Files, Mode, summary(Parity, Skips, Diffs)) :-
    candidate_mode(Candidate, Mode),
    parser_results(classic, Files, ClassicResults),
    parser_results(Candidate, Files, CandidateResults),
    compare_results(ClassicResults, CandidateResults, Candidate,
                    counts(0, 0, 0), counts(Parity, Skips, Diffs)).

candidate_mode(dcg, 'classic-vs-dcg') :-
    script_file(Script),
    file_directory_name(Script, ScriptsDir),
    directory_file_path(ScriptsDir, '../parse_dl_dcg.pl', DcgFile),
    exists_file(DcgFile),
    !.
candidate_mode(classic, 'classic-vs-classic').

parser_results(Mode, Files, Results) :-
    setup_call_cleanup(
        parity_temp_files(Manifest, Output),
        ( write_manifest(Manifest, Files),
          run_worker(Mode, Manifest, Output),
          read_results(Output, Results)
        ),
        ( delete_if_exists(Manifest), delete_if_exists(Output) )).

parity_temp_files(Manifest, Output) :-
    tmp_file(parse_parity_manifest, Manifest),
    tmp_file(parse_parity_output, Output).

write_manifest(File, Files) :-
    setup_call_cleanup(
        open(File, write, Stream),
        forall(member(Path, Files),
               write_term(Stream, file(Path), [quoted(true), fullstop(true), nl(true)])),
        close(Stream)).

run_worker(Mode, Manifest, Output) :-
    script_file(Script),
    process_create(path(swipl),
                   ['-q', '-l', Script, '-g', 'parse_parity:worker_main',
                    '-g', halt, '--', Mode, Manifest, Output],
                   [process(Pid)]),
    process_wait(Pid, Status),
    ( Status == exit(0)
    -> true
    ; throw(error(parse_parity_worker(Mode, Status), _))
    ).

worker_main :-
    current_prolog_flag(argv, [Mode, Manifest, Output]),
    configure_parser(Mode),
    parser_module_file(ParseFile),
    use_module(ParseFile, [parse_dl_file/4]),
    read_manifest(Manifest, Files),
    setup_call_cleanup(
        open(Output, write, Stream),
        forall(member(File, Files), write_parse_result(Stream, File)),
        close(Stream)).

configure_parser(dcg) :-
    setenv('DL_PARSER', dcg).
configure_parser(classic) :-
    unsetenv('DL_PARSER').

parser_module_file(ParseFile) :-
    script_file(Script),
    file_directory_name(Script, ScriptsDir),
    directory_file_path(ScriptsDir, '../parse_dl.pl', ParseFile).

read_manifest(File, Files) :-
    setup_call_cleanup(open(File, read, Stream), read_file_terms(Stream, FileTerms), close(Stream)),
    findall(Path, member(file(Path), FileTerms), Files).

write_parse_result(Stream, File) :-
    catch(( parse_dl:parse_dl_file(File, Program, _Bindings, _Findings)
          -> Outcome = success(Program)
          ; Outcome = failed
          ),
          Error,
          Outcome = throw(Error)),
    write_term(Stream, result(File, Outcome),
               [ quoted(true), fullstop(true), nl(true) ]).

read_results(File, Results) :-
    setup_call_cleanup(open(File, read, Stream), read_file_terms(Stream, Results), close(Stream)).

read_file_terms(Stream, Terms) :-
    read_term(Stream, Term, []),
    ( Term == end_of_file
    -> Terms = []
    ; Terms = [Term | Rest], read_file_terms(Stream, Rest)
    ).

compare_results([], [], _, Counts, Counts).
compare_results([result(File, Classic) | ClassicRest],
                [result(File, Candidate) | CandidateRest], Mode,
                Counts0, Counts) :-
    compare_one(File, Classic, Candidate, Mode, Kind),
    increment_count(Kind, Counts0, Counts1),
    compare_results(ClassicRest, CandidateRest, Mode, Counts1, Counts).

compare_one(File, success(_Classic), Candidate, dcg, skip) :-
    Candidate = throw(Error),
    missing_nonterminal(Error, Nonterminal),
    !,
    display_path(File, Display),
    format('SKIP ~w nonterminal=~q~n', [Display, Nonterminal]).
compare_one(File, success(Classic), success(Candidate), _, parity) :-
    Classic =@= Candidate,
    !,
    print_parity(File).
compare_one(File, throw(Classic), throw(Candidate), _, parity) :-
    Classic =@= Candidate,
    !,
    print_parity(File).
compare_one(File, Classic, Candidate, _, diff) :-
    outcome_diff(Classic, Candidate, Path, ClassicSubterm, CandidateSubterm),
    display_path(File, Display),
    format('DIFF ~w path=~q classic=~q candidate=~q~n',
           [Display, Path, ClassicSubterm, CandidateSubterm]).

print_parity(File) :-
    display_path(File, Display),
    format('PARITY ~w~n', [Display]).

outcome_diff(success(Classic), success(Candidate), Path, ClassicSubterm, CandidateSubterm) :-
    !,
    structural_diff(Classic, Candidate, Path, ClassicSubterm, CandidateSubterm).
outcome_diff(throw(Classic), throw(Candidate), Path, ClassicSubterm, CandidateSubterm) :-
    !,
    structural_diff(Classic, Candidate, Path, ClassicSubterm, CandidateSubterm).
outcome_diff(Classic, Candidate, root, Classic, Candidate).

structural_diff(Left, Right, Path, LeftSubterm, RightSubterm) :-
    canonical_term(Left, CanonicalLeft),
    canonical_term(Right, CanonicalRight),
    first_diff(CanonicalLeft, CanonicalRight, Steps, LeftSubterm, RightSubterm),
    ( Steps == [] -> Path = root ; Path = Steps ).

canonical_term(Term, Canonical) :-
    copy_term(Term, Canonical),
    numbervars(Canonical, 0, _).

first_diff(Left, Right, [], Left, Right) :-
    ( atomic(Left)
    ; atomic(Right)
    ; functor(Left, LeftName, LeftArity),
      functor(Right, RightName, RightArity),
      ( LeftName \== RightName ; LeftArity =\= RightArity )
    ),
    !.
first_diff(Left, Right, [arg(Index) | Rest], LeftSubterm, RightSubterm) :-
    functor(Left, Name, Arity),
    functor(Right, Name, Arity),
    between(1, Arity, Index),
    arg(Index, Left, LeftArg),
    arg(Index, Right, RightArg),
    LeftArg \== RightArg,
    !,
    first_diff(LeftArg, RightArg, Rest, LeftSubterm, RightSubterm).

missing_nonterminal(Term, Nonterminal) :-
    sub_term(existence_error(nonterminal, Nonterminal), Term),
    !.
missing_nonterminal(Term, Name/GrammarArity) :-
    sub_term(existence_error(procedure, Procedure), Term),
    strip_module(Procedure, _, Name/Arity),
    integer(Arity),
    Arity >= 2,
    GrammarArity is Arity - 2.

increment_count(parity, counts(P0, S, D), counts(P, S, D)) :- P is P0 + 1.
increment_count(skip, counts(P, S0, D), counts(P, S, D)) :- S is S0 + 1.
increment_count(diff, counts(P, S, D0), counts(P, S, D)) :- D is D0 + 1.

display_path(File, Display) :-
    v6_dir(V6),
    atom_concat(V6, '/', Prefix),
    ( atom_concat(Prefix, Relative, File) -> Display = Relative ; Display = File ).

v6_dir(V6) :-
    script_file(Script),
    file_directory_name(Script, ScriptsDir),
    directory_file_path(ScriptsDir, '../../..', V6Relative),
    absolute_file_name(V6Relative, V6, [file_type(directory)]).

path_from(Base, Relative, Path) :-
    directory_file_path(Base, Relative, Path).

delete_if_exists(File) :-
    ( exists_file(File) -> delete_file(File) ; true ).
