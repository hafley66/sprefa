:- begin_tests(dl7_compiler_trace).

:- use_module(library(http/json), [atom_json_dict/3]).
:- use_module(library(process), [process_create/3, process_wait/2]).
:- use_module('../src/1_libtime/0_evaluator', [stratify_rules/3]).
:- use_module('../src/2_comptime/1b_compiler_tracer').
:- use_module('../src/2_comptime/1c_compiler_cacher', [clear_compiler_caches/0]).
:- use_module('../src/2_comptime/2_compiler', [compile_dl7/4]).

trace_probe(Mode, TraceFile, Stderr) :-
    source_file(dl7_compiler_tracer:with_compile_trace(_, _), TracePath),
    atom_concat('DL7_TRACE=', Mode, TraceAssignment),
    atom_concat('DL7_TRACE_FILE=', TraceFile, FileAssignment),
    Goal = 'dl7_compiler_tracer:with_compile_trace(probe,dl7_compiler_tracer:run_compile_phase(comptime,dl7_compiler_tracer:run_compile_step(comptime,round(1),true,=([metric(rows,3)])),_))',
    process_create(
        path(env),
        [ TraceAssignment, FileAssignment,
          swipl, '-q', '-l', TracePath, '-g', Goal, '-t', halt
        ],
        [stdout(null), stderr(pipe(ErrorStream)), process(Process)]),
    read_string(ErrorStream, _, Stderr),
    close(ErrorStream),
    process_wait(Process, exit(0)).

trace_lines(Text, Prefix, Lines) :-
    split_string(Text, "\n", "", RawLines),
    include(string_starts_with(Prefix), RawLines, Lines).

string_starts_with(Prefix, Line) :-
    string_concat(Prefix, _, Line).

test(step_trace_uses_the_shared_compile_envelope) :-
    tmp_file(dl7_trace_unused, TraceFile),
    trace_probe(steps, TraceFile, Stderr),
    trace_lines(Stderr, "COMPILE-TRACE ", SummaryLines),
    trace_lines(Stderr, "COMPILE-TRACE-STEP ", StepLines),
    SummaryLines = [Summary],
    StepLines = [Step],
    sub_string(Summary, _, _, _, "program=probe"),
    sub_string(Summary, _, _, _, "comptime="),
    sub_string(Summary, _, _, _, "total="),
    sub_string(Step, _, _, _, "phase=comptime"),
    sub_string(Step, _, _, _, "step=round(1)"),
    sub_string(Step, _, _, _, "wall_ms="),
    sub_string(Step, _, _, _, "inferences="),
    sub_string(Step, _, _, _, "rows=3"),
    !.

test(json_trace_preserves_phase_step_and_metric_fields) :-
    tmp_file(dl7_trace_json, TraceFile),
    setup_call_cleanup(
        true,
        ( trace_probe(json, TraceFile, Stderr),
          trace_lines(Stderr, "COMPILE-TRACE ", [_]),
          trace_lines(Stderr, "COMPILE-TRACE-STEP ", []),
          read_file_to_string(TraceFile, Text, []),
          split_string(Text, "\n", "\n", [JsonLine]),
          atom_json_dict(JsonLine, Dict, []),
          get_dict(program, Dict, "probe"),
          get_dict(phases, Dict, [Phase]),
          get_dict(phase, Phase, "comptime"),
          get_dict(steps, Dict, [Step]),
          get_dict(step, Step, "round(1)"),
          get_dict(metrics, Step, Metrics),
          get_dict(rows, Metrics, 3)
        ),
        ( exists_file(TraceFile) -> delete_file(TraceFile) ; true )).

with_dl7_trace(Value, Goal) :-
    (   getenv('DL7_TRACE', Prior)
    ->  true
    ;   Prior = none
    ),
    setup_call_cleanup(
        setenv('DL7_TRACE', Value),
        call(Goal),
        ( restore_dl7_trace(Prior),
          reset_compile_trace )).

restore_dl7_trace(none) :-
    unsetenv('DL7_TRACE').
restore_dl7_trace(Prior) :-
    setenv('DL7_TRACE', Prior).

metrics_collected(MetricsGoal, Metrics) :-
    with_compile_trace(
        metrics_probe,
        ( run_compile_step(comptime, metric_case, true, MetricsGoal),
          collected_compile_steps([step(_, comptime, metric_case, _, Metrics)])
        )).

metrics_throw(_) :-
    throw(metrics_throw).
metrics_fail(_) :-
    fail.
metrics_malformed(not_a_list).
metrics_value([metric(kept, 1)]).

test(step_metrics_fallback_covers_throw_fail_and_malformed) :-
    with_dl7_trace('collect', (
        metrics_collected(metrics_throw, ThrowMetrics),
        metrics_collected(metrics_fail, FailMetrics),
        metrics_collected(metrics_malformed, MalformedMetrics),
        metrics_collected(metrics_value, ValueMetrics)
    )),
    ThrowMetrics == [],
    FailMetrics == [],
    MalformedMetrics == [],
    ValueMetrics == [metric(kept, 1)].

% ---------------------------------------------------------------------------
% Structured debug trace (DL7_TRACE=debug). Events are collected in-process
% before finish_compile_trace resets the ledger, or read back after the trace
% has been torn down. All env mutation is restored.

with_env(Name, Value, Goal) :-
    (   getenv(Name, Prior)
    ->  true
    ;   Prior = none
    ),
    setup_call_cleanup(
        setenv(Name, Value),
        call(Goal),
        (   Prior == none
        ->  unsetenv(Name)
        ;   setenv(Name, Prior)
        )).

debug_field(Fields, Name, Value) :-
    memberchk(Name=Value, Fields).

debug_event_name(Fields, Name) :-
    memberchk(event=Name, Fields).

debug_event_names(Events, Names) :-
    maplist(debug_event_name, Events, Names).

debug_phase_outcomes(Events, Phase, Outcomes) :-
    findall(Outcome,
            ( member(Fields, Events),
              memberchk(event=phase_end, Fields),
              memberchk(phase=Phase, Fields),
              memberchk(outcome=Outcome, Fields)
            ),
            Outcomes).

memo_debug_rules(
    [ rule(call(a, [var(value)]),
           [checked_goal(negative, call(b, [var(value)]))]),
      rule(call(b, [var(value)]), [])
    ]).

step_metrics_rows3([metric(rows, 3)]).

test(debug_mode_emits_structured_events_in_order) :-
    with_dl7_trace('debug', (
        with_compile_trace(debug_probe, (
            run_compile_phase(
                comptime,
                run_compile_step(
                    comptime, round(1), true, step_metrics_rows3),
                _),
            debug_event(custom, [phase=probe, note=hello]),
            collected_debug_events(Events))))),
    debug_event_names(
        Events,
        [scope_enter, phase_begin, step_begin, step_end, phase_end, custom]),
    [_, _, _, StepEnd, _, Custom] = Events,
    debug_field(StepEnd, outcome, success),
    debug_field(StepEnd, rows, 3),
    debug_field(Custom, note, hello).

test(debug_events_require_an_active_compile_trace) :-
    with_dl7_trace('debug', (
        reset_compile_trace,
        debug_event(orphan_event, [phase=probe]),
        aggregate_all(count,
                      dl7_compiler_tracer:compile_debug_row(_, _),
                      DebugRows))),
    DebugRows == 0.

test(off_mode_records_no_debug_events) :-
    with_dl7_trace('off', (
        with_compile_trace(debug_off_probe, (
            run_compile_phase(
                comptime, debug_event(should_not_emit, [phase=probe]), _),
            collected_debug_events(Events))))),
    Events == [].

test(debug_row_samples_are_bounded_and_opt_in) :-
    with_env('DL7_TRACE_SAMPLE_LIMIT', '2', (
        with_env('DL7_TRACE_ROWS', 'off', (
            debug_row_sample_fields(rows, [a, b, c, d, e], Bounded))))),
    debug_field(Bounded, rows_total, 5),
    debug_field(Bounded, rows_shown, 2),
    debug_field(Bounded, rows_omitted, 3),
    debug_field(Bounded, rows, none),
    with_env('DL7_TRACE_SAMPLE_LIMIT', '2', (
        with_env('DL7_TRACE_ROWS', 'all', (
            debug_row_sample_fields(rows, [a, b, c, d, e], All))))),
    debug_field(All, rows_total, 5),
    debug_field(All, rows_shown, 5),
    debug_field(All, rows_omitted, 0),
    debug_field(All, rows, [a, b, c, d, e]).

test(debug_histograms_are_bounded_and_opt_in) :-
    Pairs = [0-22, 1-11, 2-21, 3-4, 4-3],
    with_env('DL7_TRACE_SAMPLE_LIMIT', '3', (
        with_env('DL7_TRACE_ROWS', 'off', (
            debug_histogram_fields(levels, Pairs, Bounded))))),
    debug_field(Bounded, levels_total, 5),
    debug_field(Bounded, levels_shown, 3),
    debug_field(Bounded, levels_omitted, 2),
    debug_field(Bounded, levels, [0-22, 1-11, 2-21]),
    with_env('DL7_TRACE_SAMPLE_LIMIT', '3', (
        with_env('DL7_TRACE_ROWS', 'all', (
            debug_histogram_fields(levels, Pairs, All))))),
    debug_field(All, levels_total, 5),
    debug_field(All, levels_shown, 5),
    debug_field(All, levels_omitted, 0),
    debug_field(All, levels, Pairs).

test(debug_reports_memo_miss_store_and_hit) :-
    memo_debug_rules(Rules),
    with_dl7_trace('debug', (
        with_compile_trace(debug_memo_probe, (
            stratify_rules(Rules, _, _),
            stratify_rules(Rules, _, _),
            collected_debug_events(Events))))),
    findall(Action,
            ( member(Fields, Events),
              memberchk(event=memo, Fields),
              memberchk(action=Action, Fields)
            ),
            Actions),
    Actions == [miss, store, hit],
    forall(( member(Fields, Events), memberchk(event=memo, Fields) ),
           memberchk(kind=stratification, Fields)).

test(debug_phase_events_preserve_failure_and_exception) :-
    with_dl7_trace('debug', (
        with_compile_trace(debug_exit_probe, (
            (   run_compile_phase(comptime, fail, _)
            ->  FailureOutcome = unexpected
            ;   FailureOutcome = failure
            ),
            (   catch(
                    run_compile_phase(comptime, throw(debug_boom), _),
                    debug_boom, ThrowCaught = true)
            ->  true
            ;   ThrowCaught = false
            ),
            collected_debug_events(Events))))),
    FailureOutcome == failure,
    ThrowCaught == true,
    debug_phase_outcomes(Events, comptime, [failure, exception(debug_boom)]),
    aggregate_all(count, dl7_compiler_tracer:active_compile_trace(_), Active),
    Active == 0,
    compile_scope_memo_stats(0, 0, 0).

test(debug_exception_propagates_and_clears_state) :-
    catch(
        with_dl7_trace('debug', (
            with_compile_trace(debug_exception_probe, (
                debug_event(before_throw, [phase=probe]),
                throw(debug_probe_exception))))),
        debug_probe_exception,
        Caught = debug_probe_exception),
    Caught == debug_probe_exception,
    aggregate_all(count, dl7_compiler_tracer:active_compile_trace(_), Active),
    Active == 0,
    aggregate_all(count,
                  dl7_compiler_tracer:compile_debug_row(_, _),
                  DebugRows),
    DebugRows == 0,
    compile_scope_memo_stats(0, 0, 0).

test(debug_nested_scopes_are_identified_and_isolated) :-
    with_dl7_trace('debug', (
        with_compile_trace(debug_outer, (
            with_compile_trace(debug_inner, true),
            (   in_compile_scope
            ->  OuterActive = active
            ;   OuterActive = clear
            ),
            collected_debug_events(Events))))),
    OuterActive == active,
    findall(Scope,
            ( member(Fields, Events),
              memberchk(event=scope_enter, Fields),
              memberchk(scope=Scope, Fields)
            ),
            Enters),
    findall(Scope,
            ( member(Fields, Events),
              memberchk(event=scope_leave, Fields),
              memberchk(scope=Scope, Fields)
            ),
            Leaves),
    Enters = [OuterScope, InnerScope],
    OuterScope \== InnerScope,
    Leaves == [InnerScope].

test(debug_trace_preserves_compile_output) :-
    Fixture = 'v7/test/fixtures/0_minimal.dl7',
    with_dl7_trace('off', (
        clear_compiler_caches,
        compile_dl7(Fixture, RowsOff, RuntimeOff, DiagnosticsOff))),
    with_dl7_trace('debug', (
        clear_compiler_caches,
        compile_dl7(Fixture, RowsDebug, RuntimeDebug, DiagnosticsDebug))),
    RowsOff == RowsDebug,
    RuntimeOff == RuntimeDebug,
    DiagnosticsOff == DiagnosticsDebug.

debug_compile_probe(Stderr) :-
    source_file(dl7_compiler:compile_dl7(_, _, _, _), CompilerPath),
    Fixture = 'v7/test/fixtures/0_minimal.dl7',
    format(atom(Goal),
           'dl7_compiler:compile_dl7(~q,_,_,_),halt', [Fixture]),
    atom_concat('DL7_TRACE=', debug, TraceAssignment),
    process_create(
        path(env),
        [ TraceAssignment, swipl, '-q', '-s', CompilerPath,
          '-g', Goal, '-t', halt
        ],
        [stdout(null), stderr(pipe(ErrorStream)), process(Process)]),
    read_string(ErrorStream, _, Stderr),
    close(ErrorStream),
    process_wait(Process, exit(0)).

debug_line_event(Line, Event) :-
    split_string(Line, " ", " ", Fields),
    member(Field, Fields),
    string_concat("event=", EventString, Field),
    atom_string(Event, EventString).

test(debug_compile_emits_all_instrumented_boundaries) :-
    debug_compile_probe(Stderr),
    trace_lines(Stderr, "COMPILE-TRACE-DEBUG ", DebugLines),
    DebugLines \== [],
    findall(Event,
            ( member(Line, DebugLines), debug_line_event(Line, Event) ),
            Events),
    forall(
        member(Required,
               [ scope_enter, scope_leave,
                 phase_begin, phase_end, step_begin, step_end,
                 lowerer_input, lowerer_output,
                 checker_input, checker_output,
                 stratification_input, stratification_result,
                 memo, comptime_round, comptime_round_decision,
                 evaluator_install, evaluator_collect, evaluator_cleanup
               ]),
        memberchk(Required, Events)).

:- end_tests(dl7_compiler_trace).