% Step-level compile trace: the ledger, the two sinks, and the law that an
% untraced compile's stderr and emitted program are unchanged by either.

:- use_module('../../1_expansion/0_trace',
              [ dl6_trace_mode/1, reset_step_trace/0, record_step/3,
                collected_steps/1 ]).

% Every compile probe runs in a CHILD swipl. DL6_TRACE and the user_error alias
% are process-global, and plunit interleaves units across workers in one process.
trace_probe(Mode, Compiles, OutFile, TraceFile, Stderr) :-
    make_use_fixture(Dir,
        [ "main.dl6" =
            "rel person(name: text, city: text) key(1).\nrel city(name: text).\n\nbig_city_person(Name, City) <-\n  person(Name, City),\n  city(City).\n" ]),
    use_entry(Dir, 'main.dl6', Entry),
    atomic_list_concat([Dir, '/main.ts'], OutFile),
    atomic_list_concat([Dir, '/trace.jsonl'], TraceFile),
    format(atom(Goal), "compile_dl6('~w','~w')", [Entry, OutFile]),
    findall(['-g', Goal], between(1, Compiles, _), GoalPairs),
    append(GoalPairs, GoalArguments),
    trace_probe_environment(Mode, TraceFile, Assignments),
    compile_module_path(CompilePath),
    append(Assignments, [swipl, '-q', '-l', CompilePath], Head0),
    append(Head0, GoalArguments, Head),
    append(Head, ['-g', halt], Arguments),
    process_create(path(env), Arguments,
                   [ stdout(null), stderr(pipe(ErrorStream)), process(Pid) ]),
    read_string(ErrorStream, _, Stderr),
    close(ErrorStream),
    process_wait(Pid, exit(0)).

% /usr/bin/env carries the assignments rather than process_create's
% environment/1: that option replaces the whole block, PATH included.
trace_probe_environment(unset, _, ['DL6_TRACE=']).
trace_probe_environment(bogus, _, ['DL6_TRACE=bogus']).
trace_probe_environment(steps, _, ['DL6_TRACE=steps']).
trace_probe_environment(json, TraceFile, ['DL6_TRACE=json', Assignment]) :-
    atom_concat('DL6_TRACE_FILE=', TraceFile, Assignment).

compile_module_path(Path) :-
    module_property(compile, file(Path)).

trace_lines(Text, Prefix, Lines) :-
    split_string(Text, "\n", "", Raw),
    findall(Line,
            ( member(Line, Raw), string_concat(Prefix, _, Line) ),
            Lines).

% Wall values in emitted order, read back off the lines themselves.
step_walls(Lines, Walls) :-
    findall(Wall,
            ( member(Line, Lines),
              split_string(Line, " ", "", Fields),
              member(Field, Fields),
              string_concat("wall=", Digits, Field),
              number_string(Wall, Digits)
            ),
            Walls).

non_increasing([]).
non_increasing([_]) :- !.
non_increasing([First, Second | Rest]) :-
    First >= Second,
    non_increasing([Second | Rest]).

:- begin_tests(compile_step_trace).

% FAIL-FIRST RECEIPT: emitting the step lines unconditionally put 20+ extra
% lines on every compile's stderr and on every gate that reads it.
test(trace_unset_writes_no_step_line) :-
    trace_probe(unset, 1, _, _, Stderr),
    trace_lines(Stderr, "COMPILE-TRACE ", PhaseLines),
    trace_lines(Stderr, "COMPILE-TRACE-STEP ", StepLines),
    length(PhaseLines, 1),
    StepLines == [].

test(unknown_trace_name_reads_as_off) :-
    trace_probe(bogus, 1, _, _, Stderr),
    trace_lines(Stderr, "COMPILE-TRACE-STEP ", StepLines),
    StepLines == [].

test(trace_steps_keeps_the_emitted_program_byte_identical) :-
    trace_probe(unset, 1, PlainFile, _, _),
    trace_probe(steps, 1, TracedFile, _, _),
    read_file_to_string(PlainFile, Plain, []),
    read_file_to_string(TracedFile, Traced, []),
    Plain == Traced.

test(trace_steps_writes_one_phase_line_and_many_step_lines) :-
    trace_probe(steps, 1, _, _, Stderr),
    trace_lines(Stderr, "COMPILE-TRACE ", PhaseLines),
    trace_lines(Stderr, "COMPILE-TRACE-STEP ", StepLines),
    length(PhaseLines, 1),
    length(StepLines, StepCount),
    StepCount > 10,
    forall(member(Line, StepLines),
           sub_string(Line, _, _, _, "program=main phase=")).

test(trace_steps_are_sorted_by_wall_descending) :-
    trace_probe(steps, 1, _, _, Stderr),
    trace_lines(Stderr, "COMPILE-TRACE-STEP ", StepLines),
    step_walls(StepLines, Walls),
    non_increasing(Walls).

test(trace_json_appends_one_object_per_compile) :-
    trace_probe(json, 2, _, TraceFile, Stderr),
    trace_lines(Stderr, "COMPILE-TRACE-STEP ", []),
    read_file_to_string(TraceFile, Text, []),
    split_string(Text, "\n", "\n", Raw),
    exclude(==(""), Raw, Objects),
    length(Objects, 2),
    Objects = [First | _],
    atom_json_dict(First, Dict, []),
    get_dict(program, Dict, "main"),
    get_dict(phases, Dict, Phases),
    length(Phases, 6),
    get_dict(steps, Dict, [TopStep | _]),
    get_dict(step, TopStep, _),
    get_dict(phase, TopStep, _).

% The ledger is thread_local, so this one is safe beside a sibling unit.
test(ledger_sorts_by_wall_then_run_order) :-
    reset_step_trace,
    record_step(plan, slow, measurement(9, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)),
    record_step(plan, first_zero, measurement(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)),
    record_step(plan, middle, measurement(4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)),
    record_step(plan, second_zero, measurement(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)),
    collected_steps(Steps),
    findall(Name, member(step(_, Name, _, _, _, _), Steps), Names),
    Names == [slow, middle, first_zero, second_zero],
    reset_step_trace.

:- end_tests(compile_step_trace).
