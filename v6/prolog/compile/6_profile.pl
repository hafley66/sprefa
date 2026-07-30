% 6_profile.pl : opt-in phase measurements for the .dl6 compiler.
%
% The ordinary compile door does not load this module. compile_dl6.sh selects
% it only when DL_PERF_LOG names a destination, leaving the unset path on the
% exact compile:compile_dl6/2 goal it used before this file existed.
%
% Each phase is one JSON object and one line. Field names follow the v6/tsv2
% DL_PERF_LOG convention: lower snake case, elapsed values in *_ms, and one
% aggregate record per measured unit rather than per predicate call.

:- module(compile_profile,
          [ compile_dl6_profiled/2,
            execution_profile_dl6/2
          ]).

:- use_module(library(filesex), [make_directory_path/1]).
:- use_module(library(http/json), [json_write_dict/3]).
:- use_module(library(prolog_profile), [profile/2]).
:- use_module(library(tableutil), [table_statistics/2]).
:- use_module(compile, [program_plan/2]).
:- use_module(parse_dl, [parse_dl_file/4]).
:- use_module(lower, [lower_program/2, boot_statements/5]).
:- use_module(emit_ts, [emit_program/5]).

compile_dl6_profiled(File, OutFile) :-
    getenv('DL_PERF_LOG', LogFile),
    LogFile \== '',
    file_directory_name(LogFile, LogDirectory),
    make_directory_path(LogDirectory),
    setup_call_cleanup(
        open(LogFile, append, LogStream, [encoding(utf8), buffer(false)]),
        compile_dl6_profiled(File, OutFile, LogStream),
        close(LogStream)).

compile_dl6_profiled(File, OutFile, LogStream) :-
    phase(LogStream, File, 1, parse,
          parse_dl_file(File, Prog, Bindings, Findings)),
    ( Findings == []
    -> true
    ; throw(unsupported_construct(surface_findings(Findings)))
    ),
    file_base_name(File, BaseName),
    file_name_extension(Name, _Extension, BaseName),
    Term = fixture(Name, Prog, [], [], []),
    phase(LogStream, File, 2, plan,
          program_plan(Term-Bindings, Plan)),
    phase(LogStream, File, 3, lower,
          lower_program(Plan, Lowered)),
    Plan = plan(_, prog(Decls, _), RelPlans, _, _, _),
    Lowered = lowered(_, _, _, _, LevelStatements, _, _, _),
    phase(LogStream, File, 4, boot,
          boot_statements(Decls, RelPlans, [], LevelStatements,
                          BootStatements)),
    phase(LogStream, File, 5, emit,
          emit_program(Name, Plan, Lowered, BootStatements, Text)),
    phase(LogStream, File, 6, write,
          write_output(OutFile, Text)),
    format("wrote ~w~n", [OutFile]).

write_output(OutFile, Text) :-
    setup_call_cleanup(
        open(OutFile, write, Stream),
        format(Stream, "~s", [Text]),
        close(Stream)).

phase(LogStream, Source, Tick, Phase, Goal) :-
    setup_call_cleanup(
        statistics_snapshot(Before),
        phase_outcome(Goal, Outcome),
        write_phase_line(LogStream, Source, Tick, Phase, Before, Outcome)),
    restore_outcome(Outcome).

phase_outcome(Goal, Outcome) :-
    catch(
        ( once(call(Goal))
        -> Outcome = ok
        ; Outcome = failed
        ),
        Error,
        Outcome = error(Error)).

restore_outcome(ok).
restore_outcome(failed) :-
    fail.
restore_outcome(error(Error)) :-
    throw(Error).

statistics_snapshot(
        stats(CpuSeconds, Inferences, WallMilliseconds,
              GcCount, GcReclaimedBytes, GcMilliseconds, GcLeftBytes,
              TableCount, TableAnswers, TableReuses,
              TableSpaceBytes, TableCompiledSpaceBytes)) :-
    statistics(cputime, CpuSeconds),
    statistics(inferences, Inferences),
    statistics(walltime, [WallMilliseconds, _SinceLast]),
    statistics(garbage_collection,
               [GcCount, GcReclaimedBytes, GcMilliseconds, GcLeftBytes]),
    table_statistics(tables, TableCount),
    table_statistics(answers, TableAnswers),
    table_statistics(complete_call, TableReuses),
    table_statistics(space, TableSpaceBytes),
    table_statistics(compiled_space, TableCompiledSpaceBytes).

write_phase_line(LogStream, Source, Tick, Phase, Before, Outcome) :-
    statistics_snapshot(After),
    statistics_delta(Before, After,
                     WallMs, CpuMs, Inferences,
                     GcCount, GcReclaimedBytes, GcMs, GcLeftBytes,
                     TableCount, TableAnswers, TableReuses,
                     TableSpaceBytes, TableCompiledSpaceBytes),
    outcome_fields(Outcome, Status, Error),
    Line = _{
        tick: Tick,
        phase: Phase,
        source: Source,
        wall_ms: WallMs,
        cpu_ms: CpuMs,
        inferences: Inferences,
        gc_count: GcCount,
        gc_reclaimed_bytes: GcReclaimedBytes,
        gc_ms: GcMs,
        gc_left_bytes: GcLeftBytes,
        table_count: TableCount,
        table_answers: TableAnswers,
        table_reuses: TableReuses,
        table_space_bytes: TableSpaceBytes,
        table_compiled_space_bytes: TableCompiledSpaceBytes,
        status: Status,
        error: Error
    },
    json_write_dict(LogStream, Line, [width(0)]),
    nl(LogStream).

statistics_delta(
        stats(Cpu0, Inf0, Wall0, GcCount0, GcBytes0, GcMs0, _GcLeft0,
              TableCount0, TableAnswers0, TableReuses0,
              TableSpace0, TableCompiledSpace0),
        stats(Cpu1, Inf1, Wall1, GcCount1, GcBytes1, GcMs1, GcLeft1,
              TableCount1, TableAnswers1, TableReuses1,
              TableSpace1, TableCompiledSpace1),
        WallMs, CpuMs, Inferences,
        GcCount, GcReclaimedBytes, GcMs, GcLeft1,
        TableCount, TableAnswers, TableReuses,
        TableSpaceBytes, TableCompiledSpaceBytes) :-
    round_two(Wall1 - Wall0, WallMs),
    round_two((Cpu1 - Cpu0) * 1000, CpuMs),
    Inferences is Inf1 - Inf0,
    GcCount is GcCount1 - GcCount0,
    GcReclaimedBytes is GcBytes1 - GcBytes0,
    GcMs is GcMs1 - GcMs0,
    TableCount is TableCount1 - TableCount0,
    TableAnswers is TableAnswers1 - TableAnswers0,
    TableReuses is TableReuses1 - TableReuses0,
    TableSpaceBytes is TableSpace1 - TableSpace0,
    TableCompiledSpaceBytes is TableCompiledSpace1 - TableCompiledSpace0.

round_two(Value, Rounded) :-
    Rounded is round(Value * 100) / 100.

outcome_fields(ok, ok, null).
outcome_fields(failed, failed, null).
outcome_fields(error(Error), error, Message) :-
    message_to_string(Error, Message).

% Run the compiler once to settle autoloading and file-system caches, then run
% the same goal under SWI's sampling execution profiler. The textual report
% attributes self time, descendant time, calls/redos, and exits/fails.
execution_profile_dl6(File, OutFile) :-
    atom_concat(OutFile, '.warm', WarmOutFile),
    compile:compile_dl6(File, WarmOutFile),
    profile(
        compile:compile_dl6(File, OutFile),
        [ time(wall),
          sample_rate(1000),
          ports(true),
          top(30),
          cumulative(false)
        ]).
