:- module(dl7_compiler_performance, [main/0]).

:- use_module(library(http/json), [json_write_dict/3]).
:- use_module('../src/2_comptime/1b_compiler_tracer',
              [latest_compile_trace/4]).
:- use_module('../src/2_comptime/1c_compiler_cacher',
              [clear_compiler_caches/0]).
:- use_module('../src/2_comptime/2_compiler', [compile_dl7/4]).

:- initialization(main, main).

main :-
    current_prolog_flag(argv, Arguments),
    performance_fixture(Arguments, Fixture),
    setenv('DL7_TRACE', collect),
    clear_compiler_caches,
    measured_compile(Fixture, Cold, ColdOutput),
    latest_compile_trace(_, _, ColdSteps, _),
    compiler_round_count(ColdSteps, RoundCount),
    measured_compile(Fixture, Warm, WarmOutput),
    performance_report(Cold, Warm, RoundCount, Report),
    json_write_dict(current_output, Report, [width(0)]),
    nl,
    performance_failures(Cold, Warm, RoundCount,
                         ColdOutput, WarmOutput, Failures),
    report_failures(Failures),
    halt_for_failures(Failures).

performance_fixture([Fixture | _], Fixture) :- !.
performance_fixture([], 'v7/test/fixtures/2_partial.dl7').

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

compiler_round_count(Steps, Count) :-
    findall(Round,
            member(step(_, comptime, evaluate_round(Round), _, _), Steps),
            Rounds),
    length(Rounds, Count).

performance_report(
    measurement(ColdWall, ColdInferences, CompilerRows,
                RuntimeRelations, RuntimeSeeds, RuntimeRules, ColdDiagnostics),
    measurement(WarmWall, WarmInferences, _, _, _, _, WarmDiagnostics),
    RoundCount,
    _{fixture: "2_partial.dl7",
      cold: _{wall_ms: ColdWall, inferences: ColdInferences},
      warm: _{wall_ms: WarmWall, inferences: WarmInferences},
      closure_rounds: RoundCount,
      compiler_rows: CompilerRows,
      runtime: _{relations: RuntimeRelations,
                 seeds: RuntimeSeeds,
                 rules: RuntimeRules},
      diagnostics: _{cold: ColdDiagnostics, warm: WarmDiagnostics}}).

performance_failures(Cold, Warm, RoundCount,
                     ColdOutput, WarmOutput, Failures) :-
    findall(Failure,
            performance_failure(Cold, Warm, RoundCount,
                                ColdOutput, WarmOutput, Failure),
            Failures).

performance_failure(
    measurement(ColdWall, _, _, _, _, _, _), _, _, _, _,
    cold_wall_budget(ColdWall, 2000)) :-
    ColdWall > 2000.
performance_failure(
    measurement(_, ColdInferences, _, _, _, _, _), _, _, _, _,
    cold_inference_budget(ColdInferences, 15000000)) :-
    ColdInferences > 15000000.
performance_failure(
    _, measurement(WarmWall, _, _, _, _, _, _), _, _, _,
    warm_wall_budget(WarmWall, 50)) :-
    WarmWall > 50.
performance_failure(
    _, measurement(_, WarmInferences, _, _, _, _, _), _, _, _,
    warm_inference_budget(WarmInferences, 50000)) :-
    WarmInferences > 50000.
performance_failure(
    measurement(_, _, CompilerRows, _, _, _, _), _, _, _, _,
    compiler_row_checkpoint(CompilerRows, 6774)) :-
    CompilerRows =\= 6774.
performance_failure(_, _, RoundCount, _, _,
                    closure_round_checkpoint(RoundCount, 7)) :-
    RoundCount =\= 7.
performance_failure(
    measurement(_, _, _, _, _, _, Diagnostics), _, _, _, _,
    cold_diagnostics(Diagnostics)) :-
    Diagnostics \== [].
performance_failure(
    _, measurement(_, _, _, _, _, Diagnostics), _, _, _,
    warm_diagnostics(Diagnostics)) :-
    Diagnostics \== [].
performance_failure(_, _, _, ColdOutput, WarmOutput,
                    warm_output_mismatch) :-
    ColdOutput \== WarmOutput.

report_failures([]).
report_failures([Failure | Failures]) :-
    format(user_error, 'DL7-PERF-FAIL ~q~n', [Failure]),
    report_failures(Failures).

halt_for_failures([]) :- halt(0).
halt_for_failures(_) :- halt(1).
