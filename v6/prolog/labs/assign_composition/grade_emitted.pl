% grade_emitted.pl : the COMPILED half of the composition grade. For every
% fixture whose program carries a `:=`, emit the TypeScript module twice --
% once as written, once after 0_assign_expand.pl erases every `:=` -- and diff
% the two modules byte for byte.
%
% Why byte identity of the MODULE grades BOTH emitter modes at once: a single
% emitted module carries the incremental path (insertSql / supportSql) and the
% snapshot referee path (recomputeSql) side by side; SPREFA_TSV2_EMITTER_MODE
% only selects which of them the runtime executes. Two byte-identical modules
% therefore cannot differ under either setting.
%
% Run: swipl -q -l grade_emitted.pl -g grade_all -g halt

:- use_module('../../compile/compile.pl').
:- use_module('0_assign_expand', [expand_assign_program/2]).
:- use_module(library(lists)).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% prolog_load_context/2 answers only while the file is being LOADED, so the
% directory is captured into a fact here rather than read inside the runtime
% predicates (the same trap that made `just arch` cwd-dependent).
:- dynamic(lab_dir/1).
:- prolog_load_context(directory, Here), assertz(lab_dir(Here)).

fixtures_dir(Dir) :-
    lab_dir(Here),
    atomic_list_concat([Here, '/../../conformance/fixtures'], Dir).

out_dir(Dir) :-
    lab_dir(Here),
    atomic_list_concat([Here, '/out'], Dir).

grade_all :-
    out_dir(OutDir),
    ( exists_directory(OutDir) -> true ; make_directory(OutDir) ),
    findall(File-Name, assign_fixture(File, Name), Pairs),
    length(Pairs, Total),
    format("ASSIGN FIXTURES (compiled leg): ~w~n", [Total]),
    foldl(grade_one(OutDir), Pairs, tally(0,0,0), tally(Same, NameOnly, Refused)),
    format("~nRESULT byte_identical=~w column_name_only=~w refused_both=~w of ~w~n",
           [Same, NameOnly, Refused, Total]).

assign_fixture(File, Name) :-
    fixtures_dir(Dir),
    directory_files(Dir, Entries),
    member(Entry, Entries),
    file_name_extension(_, pl, Entry),
    atomic_list_concat([Dir, '/', Entry], File),
    fixture_term_in_file(File, Name).

fixture_term_in_file(File, Name) :-
    setup_call_cleanup(
        open(File, read, Stream),
        stream_fixture_name(Stream, Name),
        close(Stream)).

% Directives are CALLED as they are read, exactly as compile.pl:find_fixture/4
% does and for the same reason: op/3 declared inside a module is local to that
% module's clause parsing and does not reach a raw read_term stream, so each
% fixture file's own `:- op(1150, xfx, <-)` lines have to be replayed or every
% rule after them is a syntax error and the scan finds nothing.
stream_fixture_name(Stream, Name) :-
    repeat,
    read_term(Stream, Term, [variable_names(_)]),
    (   Term == end_of_file
    ->  !, fail
    ;   Term = (:- Directive)
    ->  catch(call(Directive), _, true), fail
    ;   Term = fixture(Candidate, prog(_, Rules), _, _, _),
        once(( member(Rule, Rules), term_has_assign(Rule) )),
        Name = Candidate
    ).

term_has_assign(Term) :-
    nonvar(Term),
    (   Term = (_ := _)
    ->  true
    ;   compound(Term),
        Term =.. [_ | Args],
        member(Arg, Args),
        term_has_assign(Arg)
    ).

grade_one(OutDir, File-Name, tally(S0,N0,R0), tally(S,N,R)) :-
    atomic_list_concat([OutDir, '/', Name, '.written.ts'],  WrittenFile),
    atomic_list_concat([OutDir, '/', Name, '.expanded.ts'], ExpandedFile),
    emit_written(File, Name, WrittenFile, WrittenOutcome),
    emit_expanded(File, Name, ExpandedFile, ExpandedOutcome),
    classify(Name, WrittenOutcome, ExpandedOutcome, WrittenFile, ExpandedFile,
             tally(S0,N0,R0), tally(S,N,R)).

emit_written(File, Name, OutFile, Outcome) :-
    catch(( compile_fixture(Name, File, OutFile), Outcome = ok ),
          Error, Outcome = refused(Error)).

emit_expanded(File, Name, OutFile, Outcome) :-
    catch(( read_fixture_term(File, Name, Term, Bindings),
            Term = fixture(Name, Program, Initial, Schedule, Expectations),
            expand_assign_program(Program, Expanded),
            compile_program(Name,
                            fixture(Name, Expanded, Initial, Schedule, Expectations),
                            Bindings, Initial, OutFile, emit_ts:emit_program),
            Outcome = ok ),
          Error, Outcome = refused(Error)).

classify(Name, refused(WrittenError), refused(ExpandedError), _, _,
         tally(S,N,R0), tally(S,N,R)) :-
    !,
    R is R0 + 1,
    (   WrittenError =@= ExpandedError
    ->  format("REFUSED_BOTH_SAME  ~w~n", [Name])
    ;   format("REFUSED_BOTH_DIFF  ~w~n  written:  ~q~n  expanded: ~q~n",
               [Name, WrittenError, ExpandedError])
    ).
classify(Name, WrittenOutcome, ExpandedOutcome, _, _, tally(S,N,R0), tally(S,N,R)) :-
    ( WrittenOutcome = refused(_) ; ExpandedOutcome = refused(_) ),
    !,
    R is R0 + 1,
    format("REFUSAL_ASYMMETRY  ~w  written=~q expanded=~q~n",
           [Name, WrittenOutcome, ExpandedOutcome]).
classify(Name, ok, ok, WrittenFile, ExpandedFile, tally(S0,N0,R), tally(S,N,R)) :-
    read_file_to_string(WrittenFile,  WrittenText,  []),
    read_file_to_string(ExpandedFile, ExpandedText, []),
    (   WrittenText == ExpandedText
    ->  S is S0 + 1, N = N0,
        format("BYTE_IDENTICAL     ~w~n", [Name])
    ;   S = S0, N is N0 + 1,
        format("COLUMN_NAME_ONLY?  ~w  (see out/~w.*.ts)~n", [Name, Name])
    ).
