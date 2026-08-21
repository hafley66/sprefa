% Step-level timing ledger for the compile driver. compile.pl owns the
% measurement machinery; this file owns only the ledger and its two sinks.

:- module(dl6_trace,
          [ dl6_trace_mode/1,
            dl6_trace_on/0,
            reset_step_trace/0,
            record_step/3,
            collected_steps/1,
            write_step_trace/2,
            run_compile_step/4,
            capture_phase_measurement/2,
            statistics_snapshot/1,
            zero_phase_measurement/1
          ]).

:- use_module(library(tableutil), [table_statistics/2]).
:- meta_predicate run_compile_step(+, +, 0, -).

:- use_module(library(lists)).
:- use_module(library(http/json), [json_write_dict/3]).

% Per-thread: two compiles on two threads must not interleave into one table.
:- thread_local step_row/4.
:- thread_local step_seq/1.
:- thread_local trace_mode_now/1.

% Resolved at load time so the default sink follows this file, never the
% caller's working directory.
:- prolog_load_context(directory, TraceDir),
   atomic_list_concat([TraceDir, '/out/compile-trace.jsonl'], DefaultFile),
   assertz(default_trace_file(DefaultFile)).

:- dynamic default_trace_file/1.

% Read fresh every call: a plunit case sets the variable inside the same
% process it then compiles in.
dl6_trace_mode(Mode) :-
    (   getenv('DL6_TRACE', Raw)
    ->  atom_string(Named, Raw)
    ;   Named = off
    ),
    (   memberchk(Named, [steps, json])
    ->  Mode = Named
    ;   Mode = off
    ).

% One env read per compile, not one per step: an untraced compile pays a single
% fact lookup at each wrapper.
dl6_trace_on :-
    (   trace_mode_now(Mode)
    ->  true
    ;   dl6_trace_mode(Mode),
        assertz(trace_mode_now(Mode))
    ),
    Mode \== off.

reset_step_trace :-
    retractall(step_row(_, _, _, _)),
    retractall(step_seq(_)),
    retractall(trace_mode_now(_)).

next_seq(Seq) :-
    (   retract(step_seq(Previous))
    ->  Seq is Previous + 1
    ;   Seq = 0
    ),
    assertz(step_seq(Seq)).

record_step(Phase, Step, Measurement) :-
    next_seq(Seq),
    assertz(step_row(Seq, Phase, Step, Measurement)).

% Wall descending, sequence number as tie-break so two 0ms steps keep run order.
collected_steps(Steps) :-
    findall(Key-step(Phase, Step, Wall, Inferences, GcMs, Tables),
            ( step_row(Seq, Phase, Step, Measurement),
              step_measurement_values(Measurement, Wall, Inferences, GcMs, Tables),
              NegativeWall is -Wall,
              Key = key(NegativeWall, Seq)
            ),
            Keyed),
    msort(Keyed, Sorted),
    findall(Row, member(_-Row, Sorted), Steps).

step_measurement_values(measurement(WallMs, _CpuMs, Inferences, _GcCount,
                                    _GcReclaimedBytes, GcMs, _GcLeftBytes,
                                    TableCount, _TableAnswers, _TableReuses,
                                    _TableSpaceBytes, _TableCompiledSpaceBytes),
                        WallMs, Inferences, GcMs, TableCount).

% Drains in every mode, so a later untraced compile cannot inherit rows left
% by an earlier traced one.
write_step_trace(Name, PhaseMeasurements) :-
    dl6_trace_mode(Mode),
    (   Mode == off
    ->  true
    ;   collected_steps(Steps),
        (   Mode == steps
        ->  write_step_lines(Name, Steps)
        ;   append_trace_json(Name, PhaseMeasurements, Steps)
        )
    ),
    reset_step_trace.

write_step_lines(Name, Steps) :-
    forall(member(step(Phase, Step, Wall, Inferences, GcMs, Tables), Steps),
           format(user_error,
                  "COMPILE-TRACE-STEP program=~w phase=~w step=~w wall=~w inf=~w gc_ms=~w tables=~w~n",
                  [Name, Phase, Step, Wall, Inferences, GcMs, Tables])).

trace_file(File) :-
    (   getenv('DL6_TRACE_FILE', Raw)
    ->  atom_string(File, Raw)
    ;   default_trace_file(File)
    ).

append_trace_json(Name, PhaseMeasurements, Steps) :-
    trace_file(File),
    file_directory_name(File, Directory),
    (   exists_directory(Directory)
    ->  true
    ;   make_directory_path(Directory)
    ),
    trace_dict(Name, PhaseMeasurements, Steps, Dict),
    setup_call_cleanup(
        open(File, append, Stream, [encoding(utf8)]),
        ( json_write_dict(Stream, Dict, [width(0)]), nl(Stream) ),
        close(Stream)).

% Wall and gc arrive as SWI rationals from round_two/2; json_write_dict has no
% rational syntax, so both cross as floats.
trace_dict(Name, PhaseMeasurements, Steps, Dict) :-
    findall(_{phase: Phase, wall: WallF, inf: Inferences,
              gc_ms: GcMsF, tables: Tables},
            ( member(phase(Phase, Measurement), PhaseMeasurements),
              step_measurement_values(Measurement, Wall, Inferences, GcMs, Tables),
              WallF is float(Wall),
              GcMsF is float(GcMs)
            ),
            PhaseDicts),
    findall(_{phase: Phase, step: StepName, wall: WallF, inf: Inferences,
              gc_ms: GcMsF, tables: Tables},
            ( member(step(Phase, Step, Wall, Inferences, GcMs, Tables), Steps),
              step_json_name(Step, StepName),
              WallF is float(Wall),
              GcMsF is float(GcMs)
            ),
            StepDicts),
    get_time(Now),
    Dict = _{program: Name, at: Now, phases: PhaseDicts, steps: StepDicts}.

% A namespaced step (expansion:option) is a compound, and json has no syntax
% for one; an atom step crosses unchanged.
step_json_name(Step, Name) :-
    (   atom(Step)
    ->  Name = Step
    ;   format(atom(Name), '~w', [Step])
    ).

% ONE step of a phase, timed and recorded when the trace is on. It lives here
% rather than in compile.pl so lower.pl can wrap its own steps: compile.pl
% imports lower, so the other direction is a module cycle.
%
% call/1 on BOTH arms, never measure_phase/3's once/1: a step that leaves a
% choice point must keep it, or a traced compile answers a different program
% from an untraced one. A step backtracked into records one row per solution.
run_compile_step(Phase, Step, Goal, Measurement) :-
    (   dl6_trace_on
    ->  statistics_snapshot(Before),
        call(Goal),
        capture_phase_measurement(Before, Measurement),
        record_step(Phase, Step, Measurement)
    ;   zero_phase_measurement(Measurement),
        call(Goal)
    ).

capture_phase_measurement(Before, Measurement) :-
    statistics_snapshot(After),
    statistics_delta(Before, After,
                     WallMs, CpuMs, Inferences,
                     GcCount, GcReclaimedBytes, GcMs, GcLeftBytes,
                     TableCount, TableAnswers, TableReuses,
                     TableSpaceBytes, TableCompiledSpaceBytes),
    Measurement = measurement(
        WallMs, CpuMs, Inferences,
        GcCount, GcReclaimedBytes, GcMs, GcLeftBytes,
        TableCount, TableAnswers, TableReuses,
        TableSpaceBytes, TableCompiledSpaceBytes).

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

zero_phase_measurement(
        measurement(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)).
