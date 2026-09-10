:- begin_tests(dl7_compiler_performance).

:- use_module(library(process), [process_create/3, process_wait/2]).
:- use_module('../bench/0_compiler_performance',
              [ fixture_profile/2,
                profile_failures/7,
                performance_exit_code/2,
                diagnostic_outcome/2,
                diagnostic_outcome/3
              ]).

nearest_shadow_profile(Profile) :-
    fixture_profile('v7/test/fixtures/lexical_binding/7_nearest_shadow.dl7',
                    Profile).

partial_profile(Profile) :-
    fixture_profile('v7/test/fixtures/2_partial.dl7', Profile).

% Boundary checks for the untraced nearest-shadow profile. A budget is
% inclusive: the value equal to the budget passes and budget+1 fails.
test(nearest_shadow_cold_boundaries) :-
    nearest_shadow_profile(Profile),
    ColdAt = measurement(3000, 16000000, 14586, 0, 0, 0, []),
    Warm = measurement(1, 2148, 14586, 0, 0, 0, []),
    profile_failures(Profile, ColdAt, Warm, 0, out, out, AtFailures),
    AtFailures == [],
    ColdOver = measurement(3001, 16000001, 14586, 0, 0, 0, []),
    profile_failures(Profile, ColdOver, Warm, 0, out, out, OverFailures),
    OverFailures == [ cold_wall_budget(3001, 3000),
                      cold_inference_budget(16000001, 16000000)
                    ].

test(nearest_shadow_row_and_diagnostic_boundaries) :-
    nearest_shadow_profile(Profile),
    Cold = measurement(1, 1, 14586, 0, 0, 0, []),
    Warm = measurement(1, 1, 14586, 0, 0, 0, []),
    profile_failures(Profile, Cold, Warm, 0, out, out, []),
    WrongRows = measurement(1, 1, 14587, 0, 0, 0, []),
    profile_failures(Profile, WrongRows, Warm, 0, out, out,
                     [compiler_row_checkpoint(14587, 14586)]),
    Diagnostic = diagnostic(evaluate, none, probe),
    BadCold = measurement(1, 1, 14586, 0, 0, 0, [Diagnostic]),
    profile_failures(Profile, BadCold, Warm, 0, out, out,
                     [cold_diagnostics([Diagnostic])]),
    BadWarm = measurement(1, 1, 14586, 0, 0, 0, [Diagnostic]),
    profile_failures(Profile, Cold, BadWarm, 0, out, out,
                     [warm_diagnostics([Diagnostic])]).

test(nearest_shadow_warm_boundaries_and_output_parity) :-
    nearest_shadow_profile(Profile),
    Cold = measurement(1, 1, 14586, 0, 0, 0, []),
    WarmAt = measurement(1, 5000, 14586, 0, 0, 0, []),
    profile_failures(Profile, Cold, WarmAt, 0, out, out, []),
    WarmOver = measurement(1, 5001, 14586, 0, 0, 0, []),
    profile_failures(Profile, Cold, WarmOver, 0, out, out,
                     [warm_inference_budget(5001, 5000)]),
    profile_failures(Profile, Cold, WarmAt, 0, out(cold), out(warm),
                     [warm_output_mismatch]).

% The 2_partial profile is preserved: same budgets, checkpoints, and order.
test(partial_boundaries) :-
    partial_profile(Profile),
    ColdAt = measurement(1, 88000000, 15542, 0, 0, 0, []),
    WarmAt = measurement(1, 50000, 15542, 0, 0, 0, []),
    profile_failures(Profile, ColdAt, WarmAt, 8, out, out, []),
    ColdOver = measurement(1, 88000001, 15543, 0, 0, 0, []),
    profile_failures(Profile, ColdOver, WarmAt, 9, out, out,
                     [ cold_inference_budget(88000001, 88000000),
                       compiler_row_checkpoint(15543, 15542),
                       closure_round_checkpoint(9, 8)
                     ]).

% Injected over-budget measurements must map to a nonzero exit, and the
% within-budget case to zero. No compile, no sleep.
test(over_budget_measurements_exit_nonzero) :-
    nearest_shadow_profile(Profile),
    Cold = measurement(9999, 99999999, 14586, 0, 0, 0, []),
    Warm = measurement(1, 2148, 14586, 0, 0, 0, []),
    profile_failures(Profile, Cold, Warm, 0, out, out, Failures),
    Failures \== [],
    performance_exit_code(Failures, Code),
    Code == 1.

test(within_budget_measurements_exit_zero) :-
    nearest_shadow_profile(Profile),
    Cold = measurement(1, 1, 14586, 0, 0, 0, []),
    Warm = measurement(1, 1, 14586, 0, 0, 0, []),
    profile_failures(Profile, Cold, Warm, 0, out, out, []),
    performance_exit_code([], 0).

% The over-budget comparator through a real process must exit nonzero.
bench_module_path(Path) :-
    source_file(dl7_compiler_performance:fixture_profile(_, _), Path).

test(over_budget_subprocess_exits_nonzero) :-
    bench_module_path(BenchPath),
    format(string(Goal),
           "use_module(~q),\c
            fixture_profile('v7/test/fixtures/lexical_binding/7_nearest_shadow.dl7', P),\c
            profile_failures(P, measurement(9999,99999999,14586,0,0,0,[]),\c
                             measurement(1,2148,14586,0,0,0,[]), 0, out, out, F),\c
            performance_exit_code(F, C), halt(C)", [BenchPath]),
    process_create(
        path(swipl),
        [ '-q', '-g', Goal, '-t', 'halt' ],
        [ stdout(null), stderr(null), process(Process) ]),
    process_wait(Process, exit(Code)),
    Code == 1.

% The actual benchmark CLI must exit nonzero on over-budget measurements and
% classify wall-only misses apart from inference misses. The --self-check modes
% inject known measurements, so there is no compile and no sleep.
contains(Text, Sub) :-
    once(sub_string(Text, _, _, _, Sub)).

run_cli(Arguments, ExitCode, Stderr) :-
    bench_module_path(BenchPath),
    process_create(
        path(swipl),
        [ '-q', '-s', BenchPath, '-g', main, '-t', 'halt', '--' | Arguments ],
        [ stdout(null), stderr(pipe(ErrorStream)), process(Process) ]),
    read_string(ErrorStream, _, Stderr),
    close(ErrorStream),
    process_wait(Process, exit(ExitCode)).

test(cli_self_check_pass_exits_zero) :-
    run_cli(['--self-check-pass'], Code, Stderr),
    Code == 0,
    contains(Stderr, "closure_rounds=null").

test(cli_self_check_wall_fails_wall_only) :-
    run_cli(['--self-check-wall'], Code, Stderr),
    Code == 1,
    contains(Stderr, "DL7-PERF-FAIL cold_wall_budget(9999,3000)"),
    contains(Stderr, "DL7-PERF-FAIL-CLASS wall_load_sensitive cold"),
    \+ contains(Stderr, "cold_inference_budget").

test(cli_self_check_inference_fails_inference_only) :-
    run_cli(['--self-check-inference'], Code, Stderr),
    Code == 1,
    contains(Stderr,
             "DL7-PERF-FAIL cold_inference_budget(99999999,16000000)"),
    contains(Stderr, "DL7-PERF-FAIL-CLASS inference_regression cold"),
    \+ contains(Stderr, "cold_wall_budget").

test(unknown_fixture_profile_fails) :-
    \+ fixture_profile('v7/test/fixtures/not_a_fixture.dl7', _).

% Matching is exact basename, not substring.
test(fixture_profile_requires_exact_basename) :-
    fixture_profile('v7/test/fixtures/lexical_binding/7_nearest_shadow.dl7', _),
    fixture_profile('v7/test/fixtures/2_partial.dl7', _),
    \+ fixture_profile('v7/test/fixtures/prefix_7_nearest_shadow.dl7.bak', _),
    \+ fixture_profile('v7/test/fixtures/2_partial.dl7.bak', _),
    \+ fixture_profile('v7/test/fixtures/not_a_fixture.dl7', _).

% The diagnostic runner is best-effort: success, ordinary failure, throw, and
% timeout all return a status and never abort the caller. No compile, no sleep.
test(diagnostic_outcome_success) :-
    diagnostic_outcome(true, Status),
    Status == success.

test(diagnostic_outcome_failure) :-
    diagnostic_outcome(fail, Status),
    Status == failure.

test(diagnostic_outcome_throw) :-
    diagnostic_outcome(throw(boom), Status),
    Status == exception(boom).

test(diagnostic_outcome_timeout) :-
    diagnostic_outcome(0.02, (repeat, fail), Status),
    Status == exception(time_limit_exceeded).

test(unknown_fixture_cli_exits_two) :-
    run_cli(['v7/test/fixtures/not_a_fixture.dl7'], Code, Stderr),
    Code == 2,
    contains(Stderr, "DL7-PERF-ERROR unknown_fixture"),
    contains(Stderr, "not_a_fixture.dl7").

:- end_tests(dl7_compiler_performance).
