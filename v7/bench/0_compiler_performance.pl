% DL7 compiler performance gate.
%
% Two fixture profiles share one measured cold/warm loop:
%
%   2_partial      traced (DL7_TRACE=collect) structural profile, unchanged:
%                  cold inference budget 88,000,000, warm 50,000, rows 15,542,
%                  eight closure rounds, empty diagnostics, warm output parity.
%   nearest-shadow timed TRACE OFF: cold wall <= 3000 ms, cold inference budget
%                  16,000,000, compiler rows 14,586, empty diagnostics, warm
%                  inference budget 5,000 (measured warm baseline 2,148), warm
%                  exact-output parity.
%
% The timed cold budget for nearest-shadow runs with tracing off. A single
% bounded diagnostic trace runs only after a failure and never changes the exit
% code. Budgets are pin-only: nothing here rewrites a baseline.

:- module(dl7_compiler_performance,
          [ main/0,
            fixture_profile/2,
            profile_failures/7,
            performance_exit_code/2,
            diagnostic_outcome/2,
            diagnostic_outcome/3
          ]).

:- use_module(library(http/json), [json_write_dict/3]).
:- use_module('../src/2_comptime/1b_compiler_tracer',
              [latest_compile_trace/4]).
:- use_module('../src/2_comptime/1c_compiler_cacher',
              [clear_compiler_caches/0]).
:- use_module('../src/2_comptime/2_compiler', [compile_dl7/4]).

% Run with `swipl -q -s 0_compiler_performance.pl -g main -t halt -- <fixture>`.
% main/0 is deliberately an initialization-free export so importing this module
% (focused tests) cannot queue the benchmark.

main :-
    current_prolog_flag(argv, Arguments),
    (   self_check_mode(Arguments, Mode)
    ->  run_self_check(Mode)
    ;   run_fixture(Arguments)
    ).

run_fixture(Arguments) :-
    (   performance_profile(Arguments, Profile)
    ->  run_profile(Profile)
    ;   report_unknown_fixture(Arguments),
        halt(2)
    ).

run_profile(Profile) :-
    Profile = profile(Label, Path, Baseline, TimedTrace, _Checks),
    report_begin(Label, Baseline, TimedTrace),
    timed_trace(TimedTrace),
    clear_compiler_caches,
    measured_compile(Path, Cold, ColdOutput),
    trace_round_count(TimedTrace, RoundCount),
    measured_compile(Path, Warm, WarmOutput),
    finish_run(Profile, Cold, Warm, RoundCount, ColdOutput, WarmOutput).

%% finish_run(+Profile, +Cold, +Warm, +RoundCount, +ColdOutput, +WarmOutput)
%% is det.
finish_run(Profile, Cold, Warm, RoundCount, ColdOutput, WarmOutput) :-
    profile_report(Profile, Cold, Warm, RoundCount, Report),
    json_write_dict(current_output, Report, [width(0)]),
    nl,
    report_lines(Report),
    profile_failures(Profile, Cold, Warm, RoundCount,
                     ColdOutput, WarmOutput, Failures),
    report_failures(Failures),
    report_failure_classes(Failures),
    diagnostic_trace_on_failure(Failures, Profile),
    performance_exit_code(Failures, ExitCode),
    halt(ExitCode).

%% self_check_mode(+Arguments, -Mode) is semidet.
self_check_mode(Arguments, Mode) :-
    member(Argument, Arguments),
    self_check_mode_name(Argument, Mode).

self_check_mode_name('--self-check-pass', pass).
self_check_mode_name('--self-check-wall', wall).
self_check_mode_name('--self-check-inference', inference).

%% run_self_check(+Mode) is det.
%
% Deterministic comparator CLI path with no compile: one known measurement set
% is injected so a subprocess can prove the real exit code and failure class.
run_self_check(Mode) :-
    fixture_profile('v7/test/fixtures/lexical_binding/7_nearest_shadow.dl7',
                    Profile),
    self_check_measurements(Mode, Cold, Warm),
    report_begin('7_nearest_shadow.dl7(self-check)',
                 'v7-compiler-perf/nearest-shadow@2026-09-10', off),
    profile_report(Profile, Cold, Warm, null, Report),
    json_write_dict(current_output, Report, [width(0)]),
    nl,
    report_lines(Report),
    profile_failures(Profile, Cold, Warm, null,
                     self_check_output, self_check_output, Failures),
    report_failures(Failures),
    report_failure_classes(Failures),
    performance_exit_code(Failures, ExitCode),
    halt(ExitCode).

self_check_measurements(pass,
                        measurement(1, 1, 14586, 0, 0, 0, []),
                        measurement(1, 1, 14586, 0, 0, 0, [])).
self_check_measurements(wall,
                        measurement(9999, 1, 14586, 0, 0, 0, []),
                        measurement(1, 1, 14586, 0, 0, 0, [])).
self_check_measurements(inference,
                        measurement(1, 99999999, 14586, 0, 0, 0, []),
                        measurement(1, 1, 14586, 0, 0, 0, [])).

report_unknown_fixture([Fixture | _]) :-
    !,
    format(user_error, 'DL7-PERF-ERROR unknown_fixture fixture=~w~n',
           [Fixture]).
report_unknown_fixture([]) :-
    format(user_error, 'DL7-PERF-ERROR unknown_fixture fixture=<none>~n', []).

%% performance_profile(+Arguments, -Profile) is det.
%
% First argument selects the fixture; no argument keeps the historical
% 2_partial default. An unrecognized fixture fails so the caller exits 2 rather
% than silently borrowing another fixture's checkpoints.
performance_profile([Fixture | _], Profile) :-
    !,
    fixture_profile(Fixture, Profile).
performance_profile([], Profile) :-
    fixture_profile('v7/test/fixtures/2_partial.dl7', Profile).

%% fixture_profile(+Path, -Profile) is semidet.
%
% Profile = profile(Label, Path, Baseline, TimedTrace, Checks). Matching is on
% the exact base name only; an unknown path fails so the caller exits 2 rather
% than silently borrowing another fixture's checkpoints.
fixture_profile(Path,
                profile('7_nearest_shadow.dl7', Path, Baseline, off, Checks)) :-
    file_base_name(Path, '7_nearest_shadow.dl7'),
    !,
    Baseline = 'v7-compiler-perf/nearest-shadow@2026-09-10',
    nearest_shadow_checks(Checks).
fixture_profile(Path,
                profile('2_partial.dl7', Path, Baseline, collect, Checks)) :-
    file_base_name(Path, '2_partial.dl7'),
    !,
    Baseline = 'v7-compiler-perf/2_partial@2026-09-10',
    partial_checks(Checks).

timed_trace(off) :-
    unsetenv('DL7_TRACE').
timed_trace(collect) :-
    setenv('DL7_TRACE', collect).

nearest_shadow_checks([
    cold_wall_limit(3000),
    cold_inference_budget(16000000),
    compiler_rows(14586),
    cold_diagnostics_empty,
    warm_diagnostics_empty,
    warm_inference_budget(5000),
    warm_output_parity
]).

partial_checks([
    cold_inference_budget(88000000),
    warm_inference_budget(50000),
    compiler_rows(15542),
    closure_rounds(8),
    cold_diagnostics_empty,
    warm_diagnostics_empty,
    warm_output_parity
]).

report_begin(Label, Baseline, TimedTrace) :-
    format(user_error,
           'DL7-PERF-BEGIN fixture=~w baseline=~w timed_trace=~w~n',
           [Label, Baseline, TimedTrace]).

measured_compile(Fixture,
                 measurement(WallMs, Inferences, CompilerRows,
                             RuntimeRelations, RuntimeSeeds, RuntimeRules,
                             Diagnostics),
                 output(Rows, Runtime, Diagnostics)) :-
    statistics(inferences, BeforeInferences),
    get_time(BeforeWall),
    compile_dl7(Fixture, Rows, Runtime, Diagnostics),
    get_time(AfterWall),
    statistics(inferences, AfterInferences),
    WallMs is round((AfterWall - BeforeWall) * 1000),
    Inferences is AfterInferences - BeforeInferences,
    length(Rows, CompilerRows),
    runtime_counts(Runtime, RuntimeRelations, RuntimeSeeds, RuntimeRules).

runtime_counts(
    checked_datalog(_, datalog_program(Relations, Seeds, Rules), _, _),
    RelationCount, SeedCount, RuleCount) :-
    !,
    length(Relations, RelationCount),
    length(Seeds, SeedCount),
    length(Rules, RuleCount).
runtime_counts(_, 0, 0, 0).

trace_round_count(off, null) :-
    !.
trace_round_count(collect, Count) :-
    latest_compile_trace(_, _, Steps, _),
    compiler_round_count(Steps, Count).

compiler_round_count(Steps, Count) :-
    findall(Round,
            member(step(_, comptime, evaluate_round(Round), _, _), Steps),
            Rounds),
    length(Rounds, Count).

profile_report(profile(Label, _Path, Baseline, TimedTrace, Checks),
               Cold, Warm, RoundCount, Report) :-
    Cold = measurement(ColdWall, ColdInf, CompilerRows,
                       RuntimeRelations, RuntimeSeeds, RuntimeRules, ColdDiag),
    Warm = measurement(WarmWall, WarmInf, _, _, _, _, WarmDiag),
    cold_regime(Checks, ColdWall, ColdInf, ColdReport),
    warm_regime(Checks, WarmWall, WarmInf, WarmReport),
    Report = _{fixture: Label,
               baseline: Baseline,
               timed_trace: TimedTrace,
               cold: ColdReport,
               warm: WarmReport,
               closure_rounds: RoundCount,
               compiler_rows: CompilerRows,
               runtime: _{relations: RuntimeRelations,
                          seeds: RuntimeSeeds,
                          rules: RuntimeRules},
               diagnostics: _{cold: ColdDiag, warm: WarmDiag}}.

cold_regime(Checks, Wall, Inf,
            _{wall_ms: Wall,
              wall_budget_ms: WallBudget,
              wall_delta_ms: WallDelta,
              inferences: Inf,
              inference_budget: InferenceBudget,
              inference_delta: InferenceDelta}) :-
    budget(cold_wall_limit, Checks, WallBudget),
    budget_delta(WallBudget, Wall, WallDelta),
    budget(cold_inference_budget, Checks, InferenceBudget),
    budget_delta(InferenceBudget, Inf, InferenceDelta).

warm_regime(Checks, Wall, Inf,
            _{wall_ms: Wall,
              wall_budget_ms: null,
              wall_delta_ms: null,
              inferences: Inf,
              inference_budget: InferenceBudget,
              inference_delta: InferenceDelta}) :-
    budget(warm_inference_budget, Checks, InferenceBudget),
    budget_delta(InferenceBudget, Inf, InferenceDelta).

budget(Key, [Check | Checks], Value) :-
    (   check_budget(Check, Key, CheckValue)
    ->  Value = CheckValue
    ;   budget(Key, Checks, Value)
    ).
budget(_, [], null).

check_budget(cold_wall_limit(N), cold_wall_limit, N).
check_budget(cold_inference_budget(N), cold_inference_budget, N).
check_budget(warm_inference_budget(N), warm_inference_budget, N).

budget_delta(null, _, null) :-
    !.
budget_delta(Budget, Actual, Delta) :-
    Delta is Actual - Budget.

report_lines(Report) :-
    get_dict(fixture, Report, Fixture),
    get_dict(baseline, Report, Baseline),
    get_dict(timed_trace, Report, TimedTrace),
    format(user_error, 'DL7-PERF fixture=~w baseline=~w timed_trace=~w~n',
           [Fixture, Baseline, TimedTrace]),
    get_dict(cold, Report, Cold),
    get_dict(warm, Report, Warm),
    report_regime(cold, Cold),
    report_regime(warm, Warm),
    get_dict(compiler_rows, Report, Rows),
    get_dict(closure_rounds, Report, Rounds),
    format(user_error, 'DL7-PERF rows=~w closure_rounds=~w~n', [Rows, Rounds]).

report_regime(Name, Regime) :-
    get_dict(wall_ms, Regime, Wall),
    get_dict(wall_budget_ms, Regime, WallBudget),
    get_dict(wall_delta_ms, Regime, WallDelta),
    get_dict(inferences, Regime, Inf),
    get_dict(inference_budget, Regime, InferenceBudget),
    get_dict(inference_delta, Regime, InferenceDelta),
    format(user_error,
           'DL7-PERF ~w wall_ms=~w budget=~w delta=~w inferences=~w budget=~w delta=~w~n',
           [Name, Wall, WallBudget, WallDelta,
            Inf, InferenceBudget, InferenceDelta]).

%% profile_failures(+Profile, +Cold, +Warm, +RoundCount,
%%                  +ColdOutput, +WarmOutput, -Failures) is det.
%
% Pure comparator: every check reads only its arguments, so tests can inject
% measurements. A budget is inclusive; a value equal to the budget passes.
profile_failures(profile(_, _, _, _, Checks), Cold, Warm, RoundCount,
                 ColdOutput, WarmOutput, Failures) :-
    findall(Failure,
            ( member(Check, Checks),
              check_failure(Check, Cold, Warm, RoundCount,
                            ColdOutput, WarmOutput, Failure) ),
            Failures).

check_failure(cold_wall_limit(Limit),
              measurement(Wall, _, _, _, _, _, _), _, _, _, _,
              cold_wall_budget(Wall, Limit)) :-
    Wall > Limit.
check_failure(cold_inference_budget(Limit),
              measurement(_, Inf, _, _, _, _, _), _, _, _, _,
              cold_inference_budget(Inf, Limit)) :-
    Inf > Limit.
check_failure(warm_inference_budget(Limit),
              _, measurement(_, Inf, _, _, _, _, _), _, _, _,
              warm_inference_budget(Inf, Limit)) :-
    Inf > Limit.
check_failure(compiler_rows(Expected),
              measurement(_, _, Rows, _, _, _, _), _, _, _, _,
              compiler_row_checkpoint(Rows, Expected)) :-
    Rows =\= Expected.
check_failure(closure_rounds(Expected), _, _, RoundCount, _, _,
              closure_round_checkpoint(RoundCount, Expected)) :-
    RoundCount =\= Expected.
check_failure(cold_diagnostics_empty,
              measurement(_, _, _, _, _, _, Diagnostics), _, _, _, _,
              cold_diagnostics(Diagnostics)) :-
    Diagnostics \== [].
check_failure(warm_diagnostics_empty,
              _, measurement(_, _, _, _, _, _, Diagnostics), _, _, _,
              warm_diagnostics(Diagnostics)) :-
    Diagnostics \== [].
check_failure(warm_output_parity, _, _, _, ColdOutput, WarmOutput,
              warm_output_mismatch) :-
    ColdOutput \== WarmOutput.

%% performance_exit_code(+Failures, -Code) is det.
performance_exit_code([], 0).
performance_exit_code([_ | _], 1).

report_failures([]).
report_failures([Failure | Failures]) :-
    format(user_error, 'DL7-PERF-FAIL ~q~n', [Failure]),
    report_failures(Failures).

%% report_failure_classes(+Failures) is det.
%
% A wall-only miss is load-sensitive, not proven extra work; an inference miss
% is the real-work regression signal. Both can appear together.
report_failure_classes(Failures) :-
    (   memberchk(cold_inference_budget(_, _), Failures)
    ->  format(user_error,
               'DL7-PERF-FAIL-CLASS inference_regression cold~n', [])
    ;   true
    ),
    (   memberchk(warm_inference_budget(_, _), Failures)
    ->  format(user_error,
               'DL7-PERF-FAIL-CLASS inference_regression warm~n', [])
    ;   true
    ),
    (   memberchk(cold_wall_budget(_, _), Failures),
        \+ memberchk(cold_inference_budget(_, _), Failures)
    ->  format(user_error,
               'DL7-PERF-FAIL-CLASS wall_load_sensitive cold~n', [])
    ;   true
    ).

%% diagnostic_trace_on_failure(+Failures, +Profile) is det.
%
% One bounded traced compile, and only when the untraced gate already failed.
% It launches at most once, is best-effort, and cannot change the exit code:
% success, ordinary failure, throw, and timeout all return with a printed
% status. Steps mode is used so the CI log carries the actual step rows.
diagnostic_trace_on_failure([], _).
diagnostic_trace_on_failure([_ | _], profile(_, Path, _, _, _)) :-
    format(user_error, 'DL7-PERF-DIAGNOSTIC start fixture=~w~n', [Path]),
    run_bounded_diagnostic_trace(Path).

%% diagnostic_outcome(+LimitSeconds, +Goal, -Status) is det.
%
% Status is success, failure, or exception(Error). A call_with_time_limit
% timeout surfaces as the exception time_limit_exceeded.
diagnostic_outcome(Limit, Goal, Status) :-
    catch(
        (   call_with_time_limit(Limit, Goal)
        ->  Status = success
        ;   Status = failure
        ),
        Error,
        Status = exception(Error)).

diagnostic_outcome(Goal, Status) :-
    diagnostic_outcome(15, Goal, Status).

run_bounded_diagnostic_trace(Path) :-
    clear_compiler_caches,
    setup_call_cleanup(
        setenv('DL7_TRACE', steps),
        diagnostic_outcome(compile_dl7(Path, _, _, _), Status),
        unsetenv('DL7_TRACE')),
    format(user_error, 'DL7-PERF-DIAGNOSTIC outcome=~q~n', [Status]).
