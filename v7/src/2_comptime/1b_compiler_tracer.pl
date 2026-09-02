% Compile timing ledger shared by every DL7 compiler entry point.

:- module(dl7_compiler_tracer,
          [ with_compile_trace/2,
            run_compile_phase/3,
            run_compile_step/4,
            reset_compile_trace/0,
            collected_compile_phases/1,
            collected_compile_steps/1
          ]).

:- use_module(library(http/json), [json_write_dict/3]).
:- use_module(library(tableutil), [table_statistics/2]).

:- meta_predicate with_compile_trace(+, 0).
:- meta_predicate run_compile_phase(+, 0, -).
:- meta_predicate run_compile_step(+, +, 0, 1).

:- thread_local active_compile_trace/1.
:- thread_local compile_phase_row/3.
:- thread_local compile_step_row/5.
:- thread_local compile_trace_sequence/1.
:- thread_local compile_trace_mode_now/1.

:- prolog_load_context(directory, TraceDirectory),
   directory_file_path(TraceDirectory, '../../out/compile-trace.jsonl',
                       DefaultTraceFile),
   assertz(default_compile_trace_file(DefaultTraceFile)).

:- dynamic default_compile_trace_file/1.

%% with_compile_trace(+Program, :Goal) is semidet.
%
% Establish one trace ledger around the outermost compiler entry point.
% Nested compiler helpers contribute phases and steps to the same ledger.
with_compile_trace(Program, Goal) :-
    (   active_compile_trace(_)
    ->  call(Goal)
    ;   setup_call_cleanup(
            begin_compile_trace(Program, Before),
            call(Goal),
            finish_compile_trace(Program, Before))
    ).

begin_compile_trace(Program, Before) :-
    reset_compile_trace,
    assertz(active_compile_trace(Program)),
    statistics_snapshot(Before).

finish_compile_trace(Program, Before) :-
    capture_measurement(Before, TotalMeasurement),
    collected_compile_phases(Phases),
    collected_compile_steps(Steps),
    write_compile_summary(Program, Phases, TotalMeasurement),
    write_compile_steps(Program, Phases, Steps),
    reset_compile_trace.

%% run_compile_phase(+Phase, :Goal, -Measurement) is semidet.
%
% Measure one named compiler phase. The cleanup records failures and thrown
% exceptions before preserving their original control flow.
run_compile_phase(Phase, Goal, Measurement) :-
    (   active_compile_trace(_)
    ->  statistics_snapshot(Before),
        call_cleanup(
            call(Goal),
            finish_compile_phase(Phase, Before, Measurement))
    ;   zero_measurement(Measurement),
        call(Goal)
    ).

finish_compile_phase(Phase, Before, Measurement) :-
    capture_measurement(Before, Measurement),
    next_compile_trace_sequence(Sequence),
    assertz(compile_phase_row(Sequence, Phase, Measurement)).

%% run_compile_step(+Phase, +Step, :Goal, :MetricsGoal) is semidet.
%
% Steps are measured only when DL7_TRACE is steps or json. MetricsGoal is
% called as MetricsGoal(-Metrics) after Goal succeeds, outside the measured
% interval. Metrics is an ordered list of metric(Name, Integer) terms.
run_compile_step(Phase, Step, Goal, MetricsGoal) :-
    (   compile_step_trace_on
    ->  statistics_snapshot(Before),
        call_cleanup(
            call(Goal),
            finish_compile_step(
                Phase, Step, Before, MetricsGoal))
    ;   call(Goal)
    ).

finish_compile_step(Phase, Step, Before, MetricsGoal) :-
    capture_measurement(Before, Measurement),
    (   catch(call(MetricsGoal, Metrics), _, fail)
    ->  true
    ;   Metrics = []
    ),
    next_compile_trace_sequence(Sequence),
    assertz(compile_step_row(
                Sequence, Phase, Step, Measurement, Metrics)).

compile_step_trace_on :-
    compile_trace_mode(Mode),
    memberchk(Mode, [steps, json]).

compile_trace_mode(Mode) :-
    (   compile_trace_mode_now(Mode)
    ->  true
    ;   read_compile_trace_mode(Mode),
        assertz(compile_trace_mode_now(Mode))
    ).

read_compile_trace_mode(Mode) :-
    (   getenv('DL7_TRACE', Raw)
    ->  atom_string(Name, Raw)
    ;   Name = off
    ),
    (   memberchk(Name, [steps, json])
    ->  Mode = Name
    ;   Mode = off
    ).

next_compile_trace_sequence(Sequence) :-
    (   retract(compile_trace_sequence(Previous))
    ->  Sequence is Previous + 1
    ;   Sequence = 0
    ),
    assertz(compile_trace_sequence(Sequence)).

reset_compile_trace :-
    retractall(active_compile_trace(_)),
    retractall(compile_phase_row(_, _, _)),
    retractall(compile_step_row(_, _, _, _, _)),
    retractall(compile_trace_sequence(_)),
    retractall(compile_trace_mode_now(_)).

collected_compile_phases(Phases) :-
    findall(Sequence-phase(Phase, Measurement),
            compile_phase_row(Sequence, Phase, Measurement),
            Keyed),
    keysort(Keyed, Sorted),
    findall(PhaseRow, member(_-PhaseRow, Sorted), Phases).

% Wall descending, sequence ascending for equal measurements.
collected_compile_steps(Steps) :-
    findall(Key-step(Sequence, Phase, Step, Measurement, Metrics),
            ( compile_step_row(
                  Sequence, Phase, Step, Measurement, Metrics),
              measurement_wall(Measurement, WallMs),
              NegativeWall is -WallMs,
              Key = key(NegativeWall, Sequence)
            ),
            Keyed),
    keysort(Keyed, Sorted),
    findall(StepRow, member(_-StepRow, Sorted), Steps).

write_compile_summary(Program, Phases, TotalMeasurement) :-
    format(user_error, 'COMPILE-TRACE program=~w', [Program]),
    forall(member(phase(Phase, Measurement), Phases),
           write_phase_field(Phase, Measurement)),
    write_phase_field(total, TotalMeasurement),
    nl(user_error).

write_phase_field(Phase, Measurement) :-
    measurement_wall_inferences(Measurement, WallMs, Inferences),
    format(user_error, ' ~w=~w/~w', [Phase, WallMs, Inferences]).

write_compile_steps(Program, Phases, Steps) :-
    compile_trace_mode(Mode),
    (   Mode == steps
    ->  maplist(write_compile_step_line(Program), Steps)
    ;   Mode == json
    ->  append_compile_trace_json(Program, Phases, Steps)
    ;   true
    ).

write_compile_step_line(
    Program, step(Sequence, Phase, Step, Measurement, Metrics)) :-
    measurement_step_values(
        Measurement, WallMs, Inferences, GcMs, TableCount),
    format(user_error,
           'COMPILE-TRACE-STEP program=~w seq=~w phase=~w step=~w wall_ms=~w inferences=~w gc_ms=~w tables=~w',
           [ Program, Sequence, Phase, Step, WallMs, Inferences,
             GcMs, TableCount
           ]),
    maplist(write_metric_field, Metrics),
    nl(user_error).

write_metric_field(metric(Name, Value)) :-
    format(user_error, ' ~w=~w', [Name, Value]).

compile_trace_file(File) :-
    (   getenv('DL7_TRACE_FILE', Raw)
    ->  atom_string(File, Raw)
    ;   default_compile_trace_file(File)
    ).

append_compile_trace_json(Program, Phases, Steps) :-
    compile_trace_file(File),
    file_directory_name(File, Directory),
    make_directory_path(Directory),
    compile_trace_dict(Program, Phases, Steps, Dict),
    setup_call_cleanup(
        open(File, append, Stream, [encoding(utf8)]),
        ( json_write_dict(Stream, Dict, [width(0)]), nl(Stream) ),
        close(Stream)).

compile_trace_dict(Program, Phases, Steps, Dict) :-
    maplist(phase_trace_dict, Phases, PhaseDicts),
    maplist(step_trace_dict, Steps, StepDicts),
    get_time(Timestamp),
    Dict = _{program: Program, at: Timestamp,
             phases: PhaseDicts, steps: StepDicts}.

phase_trace_dict(phase(Phase, Measurement), Dict) :-
    measurement_step_values(
        Measurement, WallMs, Inferences, GcMs, TableCount),
    Dict = _{phase: Phase, wall_ms: WallMs, inferences: Inferences,
             gc_ms: GcMs, tables: TableCount}.

step_trace_dict(
    step(Sequence, Phase, Step, Measurement, Metrics), Dict) :-
    measurement_step_values(
        Measurement, WallMs, Inferences, GcMs, TableCount),
    trace_name(Step, StepName),
    metrics_dict(Metrics, MetricsDict),
    Dict = _{sequence: Sequence, phase: Phase, step: StepName,
             wall_ms: WallMs, inferences: Inferences,
             gc_ms: GcMs, tables: TableCount,
             metrics: MetricsDict}.

trace_name(Name, Name) :- atom(Name), !.
trace_name(Term, Name) :- format(atom(Name), '~w', [Term]).

metrics_dict(Metrics, Dict) :-
    findall(Name-Value,
            member(metric(Name, Value), Metrics),
            Pairs),
    dict_pairs(Dict, metrics, Pairs).

measurement_wall(
    measurement(WallMs, _, _, _, _, _, _, _, _, _, _, _), WallMs).

measurement_wall_inferences(
    measurement(WallMs, _, Inferences, _, _, _, _, _, _, _, _, _),
    WallMs, Inferences).

measurement_step_values(
    measurement(WallMs, _, Inferences, _, _, GcMs, _,
                TableCount, _, _, _, _),
    WallMs, Inferences, GcMs, TableCount).

statistics_snapshot(
    stats(CpuSeconds, Inferences, WallMilliseconds,
          GcCount, GcReclaimedBytes, GcMilliseconds, GcLeftBytes,
          TableCount, TableAnswers, TableReuses,
          TableSpaceBytes, TableCompiledSpaceBytes)) :-
    statistics(cputime, CpuSeconds),
    statistics(inferences, Inferences),
    statistics(walltime, [WallMilliseconds, _]),
    statistics(garbage_collection,
               [GcCount, GcReclaimedBytes, GcMilliseconds, GcLeftBytes]),
    table_statistics(tables, TableCount),
    table_statistics(answers, TableAnswers),
    table_statistics(complete_call, TableReuses),
    table_statistics(space, TableSpaceBytes),
    table_statistics(compiled_space, TableCompiledSpaceBytes).

capture_measurement(Before, Measurement) :-
    statistics_snapshot(After),
    statistics_delta(Before, After, Measurement).

statistics_delta(
    stats(Cpu0, Inf0, Wall0, GcCount0, GcBytes0, GcMs0, _,
          TableCount0, TableAnswers0, TableReuses0,
          TableSpace0, TableCompiledSpace0),
    stats(Cpu1, Inf1, Wall1, GcCount1, GcBytes1, GcMs1, GcLeft1,
          TableCount1, TableAnswers1, TableReuses1,
          TableSpace1, TableCompiledSpace1),
    measurement(WallMs, CpuMs, Inferences,
                GcCount, GcReclaimedBytes, GcMs, GcLeft1,
                TableCount, TableAnswers, TableReuses,
                TableSpaceBytes, TableCompiledSpaceBytes)) :-
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

zero_measurement(
    measurement(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)).
