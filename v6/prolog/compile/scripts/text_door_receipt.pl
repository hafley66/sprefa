:- module(text_door_receipt, [run/0]).

:- use_module(library(filesex)).
:- use_module('../compile', [compile_dl6/2, compile_fixture/3, compile_program/6]).
:- use_module('../print_dl', [print_dl_program/3]).

:- dynamic(compile_dir_fact/1).
:- prolog_load_context(directory, Here), assertz(compile_dir_fact(Here)).

run :-
    compile_dir_fact(ScriptsDir),
    file_directory_name(ScriptsDir, CompileDir),
    atomic_list_concat([CompileDir, '/../conformance/fixtures'], FixturesDir),
    atomic_list_concat([CompileDir, '/out/text-door'], OutDir),
    make_directory_path(OutDir),
    fixture_files(FixturesDir, Files),
    findall(Status,
            ( member(File, Files), read_all_fixtures(File, Entries),
              member(entry(Name, Term, Bindings), Entries),
              grade_one(File, Name, Term, Bindings, OutDir, Status)
            ),
            Statuses),
    include(is_compiled, Statuses, Compiled),
    include(is_identical, Statuses, Identical),
    include(is_failure, Statuses, Failures),
    length(Compiled, CompiledCount),
    length(Identical, IdenticalCount),
    length(Failures, FailureCount),
    format("TEXT_DOOR compiled=~w byte_identical=~w failures=~w~n",
           [CompiledCount, IdenticalCount, FailureCount]),
    forall(member(failure(Name, Detail), Failures),
           format("  TEXT_DOOR_FAIL ~w ~q~n", [Name, Detail])),
    ( CompiledCount =:= 34, IdenticalCount =:= 34, FailureCount =:= 0
    -> true
    ; halt(1)
    ).

fixture_files(Dir, Files) :-
    directory_files(Dir, Entries),
    msort(Entries, Ordered),
    findall(Path,
            ( member(Entry, Ordered), sub_atom(Entry, _, 3, 0, '.pl'),
              atomic_list_concat([Dir, '/', Entry], Path) ),
            Files).

read_all_fixtures(File, Entries) :-
    open(File, read, Stream),
    call_cleanup(scan_fixtures(Stream, Entries), close(Stream)).

scan_fixtures(Stream, Entries) :-
    read_term(Stream, Candidate, [variable_names(Bindings)]),
    ( Candidate == end_of_file
    -> Entries = []
    ; Candidate = (:- Directive)
    -> call(Directive), scan_fixtures(Stream, Entries)
    ; Candidate = fixture(Name, _, _, _, _)
    -> Entries = [entry(Name, Candidate, Bindings) | Rest], scan_fixtures(Stream, Rest)
    ; scan_fixtures(Stream, Entries)
    ).

grade_one(File, Name, Term, Bindings, OutDir, Status) :-
    format(atom(TermOut), '~w/~w.term.ts', [OutDir, Name]),
    format(atom(SweepTermOut), '~w/~w.sweep.ts', [OutDir, Name]),
    format(atom(TextOut), '~w/~w.text.ts', [OutDir, Name]),
    format(atom(TextFile), '~w/~w.dl6', [OutDir, Name]),
    catch(
        ( quiet(compile_fixture(Name, File, SweepTermOut)),
          Term = fixture(Name, Prog, _, _, _),
          print_dl_program(Prog, Bindings, Text),
          write_text(TextFile, Text),
          quiet(compile_program(Name, fixture(Name, Prog, [], [], []), Bindings,
                                [], TermOut, emit_ts:emit_program)),
          quiet(compile_dl6(TextFile, TextOut)),
          read_file_to_string(TermOut, TermText, []),
          read_file_to_string(TextOut, TextDoorText, []),
          ( TermText == TextDoorText
          -> Status = identical(Name)
          ; Status = failure(Name, byte_difference)
          )
        ),
        Error,
        classify(Name, Error, Status)
    ).

classify(_Name, unsupported_construct(_), skipped) :- !.
classify(Name, Error, failure(Name, Error)).

write_text(File, Text) :-
    setup_call_cleanup(open(File, write, Stream),
                       format(Stream, '~w', [Text]),
                       close(Stream)).

quiet(Goal) :-
    with_output_to(string(_), Goal).

is_compiled(identical(_)).
is_compiled(failure(_, byte_difference)).
is_compiled(skipped) :- fail.

is_identical(identical(_)).
is_failure(failure(_, _)).
