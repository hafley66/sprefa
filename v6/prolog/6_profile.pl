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
:- use_module(compile,
              [ measure_phase/3,
                program_plan/2,
                restore_phase_outcome/1,
                write_compile_trace/2,
                dl6_seeded_form/3
              ]).
:- use_module('next/0_parse/use_resolve', [expand_uses/8]).
:- use_module('next/2_lower/lower', [lower_program/2, boot_statements/7]).
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
          expand_uses(File, [], [], _, Prog, _, Bindings, Findings),
          ParseMeasurement),
    ( Findings == []
    -> true
    ; throw(unsupported_construct(surface_findings(Findings)))
    ),
    file_base_name(File, BaseName),
    file_name_extension(Name, _Extension, BaseName),
    dl6_seeded_form(Prog, Initial, ProgOut),
    Term = fixture(Name, ProgOut, Initial, [], []),
    phase(LogStream, File, 2, plan,
          program_plan(Term-Bindings, Plan),
          PlanMeasurement),
    phase(LogStream, File, 3, lower,
          lower_program(Plan, Lowered),
          LowerMeasurement),
    Plan = plan(_, prog(Decls, _), Types, RelPlans, _, _, _, _, Mode),
    Lowered = lowered(_, _, _, _, LevelStatements, _, _, _),
    phase(LogStream, File, 4, boot,
          boot_statements(Mode, Decls, Types, RelPlans, Initial, LevelStatements,
                          BootStatements),
          BootMeasurement),
    phase(LogStream, File, 5, emit,
          emit_program(Name, Plan, Lowered, BootStatements, Text),
          EmitMeasurement),
    phase(LogStream, File, 6, write,
          write_output(OutFile, Text),
          WriteMeasurement),
    write_compile_trace(
        Name,
        [ phase(parse, ParseMeasurement),
          phase(plan, PlanMeasurement),
          phase(lower, LowerMeasurement),
          phase(boot, BootMeasurement),
          phase(emit, EmitMeasurement),
          phase(write, WriteMeasurement)
        ]),
    format("wrote ~w~n", [OutFile]).

write_output(OutFile, Text) :-
    setup_call_cleanup(
        open(OutFile, write, Stream),
        format(Stream, "~s", [Text]),
        close(Stream)).

phase(LogStream, Source, Tick, Phase, Goal, Measurement) :-
    measure_phase(Goal, Measurement, Outcome),
    write_phase_line(LogStream, Source, Tick, Phase, Measurement, Outcome),
    restore_phase_outcome(Outcome).

write_phase_line(LogStream, Source, Tick, Phase, Measurement, Outcome) :-
    Measurement = measurement(
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
