:- begin_tests(dl7_determinism_evidence).

:- use_module(library(http/json), [atom_json_dict/3]).
:- use_module(library(aggregate), [aggregate_all/3]).
:- use_module(library(process), [process_create/3, process_wait/2]).
:- use_module(library(prolog_wrap), [current_predicate_wrapper/4]).
:- use_module('../bench/1_determinism_evidence',
              [ classify_call/3,
                compile_fixture_report/2,
                aggregate_captured_entries/2,
                determinism_report/2,
                leftover_choicepoint/2
              ]).

test(solution_classes_and_argument_modes) :-
    classify_call(
        dl7_evaluator:integer_comparison(no_such, _, _),
        'ground-variable-variable', zero),
    classify_call(
        dl7_evaluator:stratify_rules([], _, _),
        'ground-variable-variable', one),
    classify_call(
        dl7_evaluator:integer_comparison(_, _, _),
        'variable-variable-variable', multiple).

test(successful_call_choicepoint_status) :-
    leftover_choicepoint(
        dl7_evaluator:integer_comparison(no_such, _, _), none),
    leftover_choicepoint(
        dl7_evaluator:stratify_rules([], _, _), false),
    leftover_choicepoint(
        dl7_evaluator:evaluate([], [], _, _), true),
    leftover_choicepoint(
        dl7_evaluator:integer_comparison(_, _, _), true).

test(report_aggregates_calls_and_retains_only_fingerprints) :-
    Config = evidence_config(
                 [ selected(dl7_evaluator, stratify_rules, 3),
                   selected(dl7_evaluator, integer_comparison, 3)
                 ],
                 [ recorded(first, dl7_evaluator:stratify_rules([], _, _)),
                   recorded(second, dl7_evaluator:stratify_rules([], _, _)),
                   recorded(variable, 
                            dl7_evaluator:integer_comparison(_, _, _))
                 ]),
    determinism_report(Config, report(Observations)),
    memberchk(
        observation(predicate(dl7_evaluator, stratify_rules, 3),
                    'ground-variable-variable', Fingerprint, one, false, 2),
        Observations),
    integer(Fingerprint),
    memberchk(
        observation(predicate(dl7_evaluator, integer_comparison, 3),
                    'variable-variable-variable', VariableFingerprint,
                    multiple, true, 1),
        Observations),
    integer(VariableFingerprint),
    forall(member(Observation, Observations),
           observation_has_integer_fingerprint(Observation)).

test(captured_grouping_keeps_distinct_calls_with_same_fingerprint) :-
    Entries = [
        captured_entry(
            dl7_evaluator, integer_comparison, 3,
            'ground-variable-variable', 777, pure,
            dl7_evaluator:integer_comparison(int_lt, '$VAR'(0), '$VAR'(1)),
            dl7_evaluator:integer_comparison(int_lt, _, _)),
        captured_entry(
            dl7_evaluator, integer_comparison, 3,
            'ground-variable-variable', 777, pure,
            dl7_evaluator:integer_comparison(int_gt, '$VAR'(0), '$VAR'(1)),
            dl7_evaluator:integer_comparison(int_gt, _, _))
    ],
    aggregate_captured_entries(Entries, ObservedCalls),
    length(ObservedCalls, 2),
    findall(
        Call,
        member(observed_call(
                   predicate(dl7_evaluator, integer_comparison, 3),
                   'ground-variable-variable', 777, pure, 1, Call),
               ObservedCalls),
        Calls),
    length(Calls, 2),
    Calls = [FirstCall, SecondCall],
    FirstCall \== SecondCall.

observation_has_integer_fingerprint(
    observation(_, _, Fingerprint, _, _, _)) :-
    integer(Fingerprint).

run_evidence(Arguments, ExitCode, Stdout, Stderr) :-
    process_create(
        path(bash),
        ['scripts/v7-determinism-evidence.sh' | Arguments],
        [stdout(pipe(OutStream)), stderr(pipe(ErrorStream)), process(Process)]),
    read_string(OutStream, _, Stdout),
    close(OutStream),
    read_string(ErrorStream, _, Stderr),
    close(ErrorStream),
    process_wait(Process, exit(ExitCode)).

test(machine_output_and_text_table_are_deterministic) :-
    run_evidence([], CodeOne, StdoutOne, StderrOne),
    run_evidence([], CodeTwo, StdoutTwo, StderrTwo),
    CodeOne == 0,
    CodeTwo == 0,
    StdoutOne == StdoutTwo,
    determinism_table_lines(StderrOne, TableOne),
    determinism_table_lines(StderrTwo, TableTwo),
    TableOne == TableTwo,
    atom_string(JsonAtom, StdoutOne),
    atom_json_dict(JsonAtom, Dict, []),
    get_dict(schema, Dict, "dl7-determinism-evidence-v1"),
    get_dict(observed_calls, Dict, Observations),
    length(Observations, ObservedCount),
    ObservedCount > 1,
    memberchk("DL7-DETERMINISM predicate | groundness | calls | replayability | fingerprint",
              TableOne).

determinism_table_lines(Stderr, Lines) :-
    split_string(Stderr, "\n", "", RawLines),
    include(determinism_line, RawLines, Lines).

determinism_line(Line) :-
    string_concat("DL7-DETERMINISM", _, Line).

test(unknown_predicate_and_invalid_config_fail_nonzero) :-
    run_evidence(['--unknown-predicate'], UnknownCode, _, UnknownError),
    UnknownCode == 2,
    once(sub_string(UnknownError, _, _, _, "DL7-DETERMINISM-ERROR")),
    run_evidence(['--invalid-config'], ConfigCode, _, ConfigError),
    ConfigCode == 2,
    once(sub_string(ConfigError, _, _, _, "DL7-DETERMINISM-ERROR")).

test(unknown_predicate_report_fails_before_calling) :-
    Config = evidence_config(
                 [selected(dl7_determinism_evidence, no_such_predicate, 0)],
                 []),
    catch(
        determinism_report(Config, _),
        Error,
        true),
    Error = error(existence_error(predicate,
                                  dl7_determinism_evidence:no_such_predicate/0),
                  _).

test(real_fixture_observes_repeated_calls_and_non_replayable_entries) :-
    Fixture = 'v7/test/fixtures/lexical_binding/7_nearest_shadow.dl7',
    compile_fixture_report(
        Fixture,
        fixture_report(Fixture, success, ObservedCalls, UniqueInputs,
                       ReplayedInputs, Classifications)),
    UniqueInputs > 1,
    ReplayedInputs > 0,
    memberchk(
        observed_call(predicate(dl7_evaluator, integer_comparison, 3),
                      _, _, pure, RepeatedCount, _),
        ObservedCalls),
    RepeatedCount > 1,
    memberchk(
        observed_call(predicate(dl7_evaluator, evaluate, 4),
                      _, _, non_replayable, _, _),
        ObservedCalls),
    memberchk(
        classification(predicate(dl7_evaluator, evaluate, 4),
                       _, _, _, non_replayable, none, none),
        Classifications),
    capture_wrapper_count(0).

test(wrapper_cleanup_runs_after_compile_exception) :-
    catch(
        compile_fixture_report('v7/test/fixtures/no_such_fixture.dl7', _),
        _,
        true),
    capture_wrapper_count(0).

capture_wrapper_count(Expected) :-
    aggregate_all(
        count,
        current_predicate_wrapper(
            dl7_evaluator:integer_comparison(_, _, _),
            dl7_determinism_evidence_capture, _, _),
        Expected).

:- end_tests(dl7_determinism_evidence).
