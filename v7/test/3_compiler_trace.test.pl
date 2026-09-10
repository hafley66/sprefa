:- begin_tests(dl7_compiler_trace).

:- use_module(library(http/json), [atom_json_dict/3]).
:- use_module(library(process), [process_create/3, process_wait/2]).
:- use_module('../src/2_comptime/1b_compiler_tracer').

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

:- end_tests(dl7_compiler_trace).
