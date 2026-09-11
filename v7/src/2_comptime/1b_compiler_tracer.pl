% Compile timing ledger shared by every DL7 compiler entry point.

:- module(dl7_compiler_tracer,
          [ with_compile_trace/2,
            run_compile_phase/3,
            run_compile_step/4,
            reset_compile_trace/0,
            collected_compile_phases/1,
            collected_compile_steps/1,
            collected_debug_events/1,
            latest_compile_trace/4,
            in_compile_scope/0,
            compile_scope_memo_lookup/3,
            compile_scope_memo_store/3,
            compile_scope_memo_stats/3,
            debug_trace_on/0,
            debug_event/2,
            debug_histogram_fields/3,
            debug_row_sample_fields/3,
            debug_sample_limit/1,
            debug_rows_all/0
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
:- thread_local latest_compile_trace/4.
:- thread_local compile_scope_frames/1.
:- thread_local compile_scope_frame_counter/1.
:- thread_local compile_scope_memo/4.
:- thread_local compile_scope_memo_stat/3.
:- thread_local compile_debug_row/2.

:- prolog_load_context(directory, TraceDirectory),
   directory_file_path(TraceDirectory, '../../out/compile-trace.jsonl',
                       DefaultTraceFile),
   assertz(default_compile_trace_file(DefaultTraceFile)).

:- dynamic default_compile_trace_file/1.

%% with_compile_trace(+Program, :Goal) is semidet.
%
% Establish one trace ledger around the outermost compiler entry point.
% Nested compiler helpers contribute phases and steps to the same ledger.
% Every call also opens a fresh compile-scope memo frame, so a nested
% compilation cannot read or erase another active compilation's entries.
with_compile_trace(Program, Goal) :-
    (   active_compile_trace(_)
    ->  with_compile_scope_frame(Goal)
    ;   setup_call_cleanup(
            begin_compile_trace(Program, Before),
            call(Goal),
            finish_compile_trace(Program, Before))
    ).

with_compile_scope_frame(Goal) :-
    open_compile_scope_frame,
    setup_call_cleanup(true, call(Goal), close_compile_scope_frame).

begin_compile_trace(Program, Before) :-
    reset_compile_trace,
    retractall(latest_compile_trace(_, _, _, _)),
    assertz(active_compile_trace(Program)),
    open_compile_scope_frame,
    statistics_snapshot(Before).

finish_compile_trace(Program, Before) :-
    close_compile_scope_frame,
    capture_measurement(Before, TotalMeasurement),
    collected_compile_phases(Phases),
    collected_compile_steps(Steps),
    assertz(latest_compile_trace(
                Program, Phases, Steps, TotalMeasurement)),
    write_compile_debug,
    write_compile_summary(Program, Phases, TotalMeasurement),
    write_compile_steps(Program, Phases, Steps),
    reset_compile_trace.

%% Compile-scope memo frames. A frame is the lifetime of one with_compile_trace
%% call; its entries are thread local and visible only through the top frame, so
%% nested scopes stay isolated and concurrent threads never share an entry.
open_compile_scope_frame :-
    (   retract(compile_scope_frame_counter(Previous))
    ->  Frame is Previous + 1
    ;   Frame = 1
    ),
    assertz(compile_scope_frame_counter(Frame)),
    (   retract(compile_scope_frames(Frames0))
    ->  Frames = [Frame | Frames0]
    ;   Frames = [Frame]
    ),
    assertz(compile_scope_frames(Frames)),
    assertz(compile_scope_memo_stat(Frame, 0, 0)),
    length(Frames, Depth),
    debug_event(scope_enter, [depth=Depth]).

close_compile_scope_frame :-
    (   compile_scope_frames([Frame | Rest])
    ->  debug_event(scope_leave, []),
        retract(compile_scope_frames([Frame | Rest])),
        retractall(compile_scope_memo(Frame, _, _, _)),
        retractall(compile_scope_memo_stat(Frame, _, _)),
        assertz(compile_scope_frames(Rest))
    ;   true
    ).

%% in_compile_scope is semidet.
in_compile_scope :-
    compile_scope_frames([_ | _]).

%% compile_scope_memo_store(+Hash, +Key, +Value) is det.
%
% Store one memo entry in the current frame. Hash is a caller-chosen bucket
% selector; the full Key is retained so lookup can verify it exactly.
compile_scope_memo_store(Hash, Key, Value) :-
    compile_scope_frames([Frame | _]),
    assertz(compile_scope_memo(Frame, Hash, Key, Value)),
    debug_memo_event(store, Key, Value).

%% compile_scope_memo_lookup(+Hash, +Key, -Value) is semidet.
%
% Select the Hash bucket, then unify only entries whose stored Key is the exact
% same term. A hash collision therefore cannot produce a false hit.
compile_scope_memo_lookup(Hash, Key, Value) :-
    compile_scope_frames([Frame | _]),
    (   compile_scope_memo(Frame, Hash, StoredKey, Value),
        StoredKey == Key
    ->  compile_scope_memo_hit(Frame),
        debug_memo_event(hit, Key, Value)
    ;   compile_scope_memo_miss(Frame),
        debug_memo_event(miss, Key, none),
        fail
    ).

compile_scope_memo_hit(Frame) :-
    (   retract(compile_scope_memo_stat(Frame, Hits0, Misses))
    ->  Hits is Hits0 + 1,
        assertz(compile_scope_memo_stat(Frame, Hits, Misses))
    ;   true
    ).

compile_scope_memo_miss(Frame) :-
    (   retract(compile_scope_memo_stat(Frame, Hits, Misses0))
    ->  Misses is Misses0 + 1,
        assertz(compile_scope_memo_stat(Frame, Hits, Misses))
    ;   true
    ).

%% compile_scope_memo_stats(-Hits, -Misses, -Entries) is det.
%
% Current-frame memo counters, or all zero outside a compile scope.
compile_scope_memo_stats(Hits, Misses, Entries) :-
    (   compile_scope_frames([Frame | _])
    ->  (   compile_scope_memo_stat(Frame, FrameHits, FrameMisses)
        ->  Hits = FrameHits,
            Misses = FrameMisses
        ;   Hits = 0,
            Misses = 0
        ),
        findall(Key, compile_scope_memo(Frame, _, Key, _), Keys),
        length(Keys, Entries)
    ;   Hits = 0,
        Misses = 0,
        Entries = 0
    ).

%% debug_memo_event(+Action, +Key, +Value) is det.
%
% Emit one structured memo event under debug tracing. Emission is best effort:
% a failure or throw in field derivation is swallowed so the memo lookup or
% store that triggered it keeps its original result.
debug_memo_event(Action, Key, Value) :-
    (   debug_trace_on
    ->  (   catch(( memo_kind_fields(Key, Value, Fields),
                    debug_event(memo, [phase=memo, action=Action | Fields]),
                    true ), _, true)
        ->  true
        ;   true
        )
    ;   true
    ).

memo_kind_fields(Key, Value, Fields) :-
    (   Key = stratification(Rules),
        is_list(Rules)
    ->  Kind = stratification,
        length(Rules, KeySize)
    ;   Kind = memo,
        KeySize = 1
    ),
    term_hash(Key, Digest),
    memo_value_shape(Value, Shape, ValueCount),
    Fields = [kind=Kind, digest=Digest, key_size=KeySize,
              value_shape=Shape, value_count=ValueCount].

memo_value_shape(Value, Shape, Count) :-
    (   Value == none
    ->  Shape = miss,
        Count = 0
    ;   Value = stratification_result(Strata, Diagnostics)
    ->  length(Strata, StrataCount),
        length(Diagnostics, DiagnosticCount),
        Shape = strata_diagnostics,
        Count is StrataCount + DiagnosticCount
    ;   compound(Value)
    ->  functor(Value, Name, Arity),
        Shape = term(Name / Arity),
        Count = 1
    ;   Shape = scalar,
        Count = 1
    ).

%% debug_trace_on is semidet.
%
% True only inside an active compile trace whose mode is debug.
debug_trace_on :-
    active_compile_trace(_),
    compile_trace_mode(debug).

%% debug_event(+Event, +Fields) is det.
%
% Record one structured debug event. Sequence number and current scope id are
% prepended; the row is emitted by write_compile_debug/0 at compile finish in
% sequence order. Off-trace calls and any field derivation failure are inert.
debug_event(Event, Fields) :-
    (   debug_trace_on
    ->  catch(record_debug_event(Event, Fields), _, true)
    ;   true
    ).

record_debug_event(Event, Fields) :-
    next_compile_trace_sequence(Sequence),
    (   compile_scope_frames([Frame | _])
    ->  Scope = Frame
    ;   Scope = none
    ),
    assertz(compile_debug_row(
                Sequence, [seq=Sequence, event=Event, scope=Scope | Fields])).

collected_debug_events(Events) :-
    findall(Sequence-Fields, compile_debug_row(Sequence, Fields), Keyed),
    keysort(Keyed, Sorted),
    findall(Fields, member(_-Fields, Sorted), Events).

%% debug_sample_limit(-Limit) is det.
%
% Bounded sample window for debug relation samples and histograms. Defaults to
% a small integer and is configurable through DL7_TRACE_SAMPLE_LIMIT.
debug_sample_limit(Limit) :-
    (   getenv('DL7_TRACE_SAMPLE_LIMIT', Raw)
    ->  (   catch(atom_number(Raw, Number), _, fail),
            integer(Number),
            Number >= 0
        ->  Limit = Number
        ;   Limit = 5
        )
    ;   Limit = 5
    ).

%% debug_rows_all is semidet.
%
% True when the operator explicitly opted into full row terms.
debug_rows_all :-
    getenv('DL7_TRACE_ROWS', Raw),
    Raw == all.

%% debug_histogram_fields(+Label, +Pairs, -Fields) is det.
%
% Bounded histogram sample over structural key-count pairs. Default prints the
% counts and at most DL7_TRACE_SAMPLE_LIMIT key-count pairs; DL7_TRACE_ROWS=all
% prints every pair. Pairs must already be in a deterministic order.
debug_histogram_fields(Label, Pairs, Fields) :-
    length(Pairs, Buckets),
    debug_sample_limit(Limit),
    (   debug_rows_all
    ->  Sample = Pairs
    ;   take_first(Pairs, Limit, Sample)
    ),
    length(Sample, Shown),
    Omitted is Buckets - Shown,
    sample_field_names(Label,
                       BucketsName, ShownName, OmittedName),
    Fields = [ BucketsName = Buckets,
               ShownName = Shown,
               OmittedName = Omitted,
               Label = Sample
             ].

%% debug_row_sample_fields(+Label, +Rows, -Fields) is det.
%
% Counts for one possibly large row list. Row terms are withheld unless the
% operator opted in with DL7_TRACE_ROWS=all; otherwise only total, bounded
% sample size, and omitted counts are reported.
debug_row_sample_fields(Label, Rows, Fields) :-
    length(Rows, Total),
    debug_sample_limit(Limit),
    take_first(Rows, Limit, Bounded),
    length(Bounded, Shown),
    Omitted is Total - Shown,
    (   debug_rows_all
    ->  Sample = Rows,
        AllShown = Total,
        AllOmitted = 0
    ;   Sample = none,
        AllShown = Shown,
        AllOmitted = Omitted
    ),
    sample_field_names(Label,
                       TotalName, ShownName, OmittedName),
    Fields = [ TotalName = Total,
               ShownName = AllShown,
               OmittedName = AllOmitted,
               Label = Sample
             ].

take_first(List, Limit, First) :-
    take_first_(List, Limit, First).

take_first_([], _, []) :-
    !.
take_first_(_, 0, []) :-
    !.
take_first_([Head | Tail], Limit, [Head | First]) :-
    NextLimit is Limit - 1,
    take_first_(Tail, NextLimit, First).

sample_field_names(Label, TotalName, ShownName, OmittedName) :-
    atom_concat(Label, '_total', TotalName),
    atom_concat(Label, '_shown', ShownName),
    atom_concat(Label, '_omitted', OmittedName).

%% run_compile_phase(+Phase, :Goal, -Measurement) is semidet.
%
% Measure one named compiler phase. The cleanup records failures and thrown
% exceptions before preserving their original control flow.
run_compile_phase(Phase, Goal, Measurement) :-
    (   active_compile_trace(_)
    ->  statistics_snapshot(Before),
        (   compile_trace_mode(debug)
        ->  run_debug_phase(Phase, Goal, Before, Measurement)
        ;   call_cleanup(
                call(Goal),
                finish_compile_phase(Phase, Before, Measurement))
        )
    ;   zero_measurement(Measurement),
        call(Goal)
    ).

%% run_debug_phase(+Phase, :Goal, +Before, -Measurement) is semidet.
%
% Debug-mode phase execution. Begin/end events carry the outcome; the phase row
% is recorded exactly as in the other trace modes, and the original control flow
% (success, failure, or the original exception term) is preserved.
run_debug_phase(Phase, Goal, Before, Measurement) :-
    debug_event(phase_begin, [phase=Phase]),
    catch(
        (   call(Goal)
        ->  Outcome = success
        ;   Outcome = failure
        ),
        Error,
        Outcome = exception(Error)),
    finish_compile_phase(Phase, Before, Measurement),
    measurement_wall_inferences(Measurement, WallMs, Inferences),
    debug_event(phase_end, [phase=Phase, outcome=Outcome,
                            wall_ms=WallMs, inferences=Inferences]),
    (   Outcome == success
    ->  true
    ;   Outcome == failure
    ->  fail
    ;   Outcome = exception(Exception),
        throw(Exception)
    ).

finish_compile_phase(Phase, Before, Measurement) :-
    capture_measurement(Before, Measurement),
    next_compile_trace_sequence(Sequence),
    assertz(compile_phase_row(Sequence, Phase, Measurement)).

%% run_compile_step(+Phase, +Step, :Goal, :MetricsGoal) is semidet.
%
% Steps are measured when DL7_TRACE is steps, json, or collect. MetricsGoal is
% called as MetricsGoal(-Metrics) after Goal succeeds, outside the measured
% interval. Metrics is an ordered list of metric(Name, Integer) terms.
run_compile_step(Phase, Step, Goal, MetricsGoal) :-
    (   compile_step_trace_on
    ->  statistics_snapshot(Before),
        call_cleanup(
            call(Goal),
            finish_compile_step(
                Phase, Step, Before, MetricsGoal))
    ;   compile_trace_mode(debug)
    ->  run_debug_step(Phase, Step, Goal, MetricsGoal)
    ;   call(Goal)
    ).

%% run_debug_step(+Phase, +Step, :Goal, :MetricsGoal) is semidet.
%
% Debug-mode step execution. Metrics are gathered after the timed interval, and
% begin/end events carry outcome and the same metric fields the step writers
% would report. Failure and exceptions keep their original control flow.
run_debug_step(Phase, Step, Goal, MetricsGoal) :-
    statistics_snapshot(Before),
    debug_event(step_begin, [phase=Phase, step=Step]),
    catch(
        (   call(Goal)
        ->  Outcome = success
        ;   Outcome = failure
        ),
        Error,
        Outcome = exception(Error)),
    capture_measurement(Before, Measurement),
    measurement_step_values(
        Measurement, WallMs, Inferences, GcMs, TableCount),
    collect_step_metrics(MetricsGoal, Metrics),
    metrics_fields(Metrics, MetricFields),
    debug_event(step_end, [phase=Phase, step=Step, outcome=Outcome,
                           wall_ms=WallMs, inferences=Inferences,
                           gc_ms=GcMs, tables=TableCount | MetricFields]),
    (   Outcome == success
    ->  true
    ;   Outcome == failure
    ->  fail
    ;   Outcome = exception(Exception),
        throw(Exception)
    ).

collect_step_metrics(MetricsGoal, Metrics) :-
    (   catch(call(MetricsGoal, Collected), _, fail),
        is_list(Collected)
    ->  Metrics = Collected
    ;   Metrics = []
    ).

metrics_fields(Metrics, Fields) :-
    findall(Name=Value, member(metric(Name, Value), Metrics), Fields).

finish_compile_step(Phase, Step, Before, MetricsGoal) :-
    capture_measurement(Before, Measurement),
    collect_step_metrics(MetricsGoal, Metrics),
    next_compile_trace_sequence(Sequence),
    assertz(compile_step_row(
                Sequence, Phase, Step, Measurement, Metrics)).

compile_step_trace_on :-
    active_compile_trace(_),
    compile_trace_mode(Mode),
    memberchk(Mode, [steps, json, collect]).

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
    (   memberchk(Name, [steps, json, collect, debug])
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
    retractall(compile_trace_mode_now(_)),
    retractall(compile_scope_frames(_)),
    retractall(compile_scope_frame_counter(_)),
    retractall(compile_scope_memo(_, _, _, _)),
    retractall(compile_scope_memo_stat(_, _, _)),
    retractall(compile_debug_row(_, _)).

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

%% write_compile_debug is det.
%
% Emit collected debug events in sequence order. Only debug mode writes; other
% modes keep their existing writers untouched.
write_compile_debug :-
    (   compile_trace_mode(debug)
    ->  collected_debug_events(Events),
        maplist(write_debug_line, Events)
    ;   true
    ).

write_debug_line(Fields) :-
    format(user_error, 'COMPILE-TRACE-DEBUG', []),
    forall(member(Name=Value, Fields),
           format(user_error, ' ~w=~w', [Name, Value])),
    nl(user_error).

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
