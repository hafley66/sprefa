% Step-level timing ledger for the compile driver. compile.pl owns the
% measurement machinery; this file owns only the ledger and its two sinks.

:- module(dl6_trace,
          [ dl6_trace_mode/1,
            dl6_trace_on/0,
            reset_step_trace/0,
            record_step/3,
            collected_steps/1,
            write_step_trace/2
          ]).

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
    findall(_{phase: Phase, step: Step, wall: WallF, inf: Inferences,
              gc_ms: GcMsF, tables: Tables},
            ( member(step(Phase, Step, Wall, Inferences, GcMs, Tables), Steps),
              WallF is float(Wall),
              GcMsF is float(GcMs)
            ),
            StepDicts),
    get_time(Now),
    Dict = _{program: Name, at: Now, phases: PhaseDicts, steps: StepDicts}.
