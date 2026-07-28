% oracle_dump.pl : Phase C's oracle side. Loads conformance/ticklog.pl
% UNCHANGED (which itself loads go.pl -> engine.pl + every fixtures/*.pl,
% also unchanged), then for EVERY fixture(Name, Prog, Initial, Schedule, _)
% fact runs print_ticklog/3 (ticklog.pl's own predicate, called exactly as
% ticklog.pl:emit/1 does) captured to a string rather than printed straight
% to stdout, and writes it to out/<name>.oracle.jsonl. A fixture whose own
% run_program/5 throws (several engine_core.pl fixtures deliberately exercise
% an engine rejection path, e.g. log_retraction_rejected) is caught and
% reported as ORACLE_THROW rather than aborting the whole dump.
%
% Deliberately its own swipl process, separate from sweep.pl (the compile
% side): ticklog.pl's own header says it "NEVER edits engine.pl, go.pl,
% body.pl, level_eval.pl, or any fixtures/*.pl" by loading them the normal
% consult way; sweep.pl reads those same fixture files a SECOND, different
% way (raw read_term, no consult, to keep surface variable identity) that
% must not share a process with a consult of the same files.
%
% No `:- module(...)` header, matching ticklog.pl/go.pl themselves: this
% file has to call fixture/5, print_ticklog/3, and run_program/5 unqualified,
% which only works loaded into the same (user) context ticklog.pl loads into.
%
% Run: swipl -q -l v6/prolog/compile/oracle_dump.pl -g dump_all -g halt

:- ensure_loaded('../conformance/ticklog').

:- dynamic(oracle_dump_dir_fact/1).
:- prolog_load_context(directory, Here), atomic_list_concat([Here, '/out'], OutDir),
   assertz(oracle_dump_dir_fact(OutDir)).

dump_all :-
    oracle_dump_dir_fact(OutDir),
    ( exists_directory(OutDir) -> true ; make_directory(OutDir) ),
    findall(Name, fixture(Name, _, _, _, _), Names),
    forall(member(Name, Names), dump_one(OutDir, Name)).

dump_one(OutDir, Name) :-
    catch(
        ( fixture(Name, Prog, Initial, Schedule, _Expectations),
          with_output_to(string(Text), print_ticklog(Prog, Initial, Schedule)),
          format(atom(OutFile), '~w/~w.oracle.jsonl', [OutDir, Name]),
          setup_call_cleanup(open(OutFile, write, Stream), format(Stream, "~w", [Text]), close(Stream)),
          format("ORACLE_OK ~w~n", [Name])
        ),
        Error,
        format("ORACLE_THROW ~w ~q~n", [Name, Error])
    ).
