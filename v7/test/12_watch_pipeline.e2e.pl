:- begin_tests(dl7_watch_pipeline).

:- use_module(library(http/json), [atom_json_dict/3, json_write_dict/3]).
:- use_module(library(process), [process_create/3, process_wait/2]).
:- use_module('../src/2_comptime/2_compiler', [compile_dl7_project_rows/6]).
:- use_module('../src/3_emit/1a_dbsp_plan_emitter', [emit_dbsp_plan/3]).
:- use_module('../src/4_tool/0_source_query_mainer', [extract_tsi_rows/4]).

test(checkout_snapshot_flows_through_the_resident_dl7_program) :-
    tmp_file_stream(text, PlanPath, PlanStream),
    atom_concat(PlanPath, '.receipts.sqlite3', StatePath),
    setup_call_cleanup(
        true,
        watch_pipeline(PlanPath, PlanStream, StatePath, Observed),
        cleanup_watch_files(PlanPath, StatePath)),
    Observed == watch_result(
                    extract(exit(0)),
                    runner(exit(0)),
                    tick(0),
                    traits([["Render"]])).

watch_pipeline(PlanPath, PlanStream, StatePath, Observed) :-
    Program = 'v7/examples/0_rust_traits.dl7',
    Source = 'v6/sprefa-extract/tests/fixtures/tsi/probe_graph.rs',
    Extract = 'v6/sprefa-extract/target/debug/extract',
    Runner = 'v6/dd-runner/target/debug/dd-runner',
    extract_tsi_rows(Extract, Source, TsiRows, []),
    once(absolute_file_name(Program, AbsoluteProgram,
                            [access(read), file_errors(error)])),
    file_directory_name(AbsoluteProgram, Root),
    compile_dl7_project_rows(
        Root, [AbsoluteProgram], TsiRows,
        _, Runtime, CompileDiagnostics),
    CompileDiagnostics == [],
    emit_dbsp_plan(Runtime, Plan, EmitDiagnostics),
    EmitDiagnostics == [],
    json_write_dict(PlanStream, Plan, [width(0)]),
    nl(PlanStream),
    close(PlanStream),
    run_process(
        Extract,
        [ watch, '.', '--pattern', Source, '--family', type,
          '--state', StatePath, '--once'
        ],
        "", WatchOutput, ExtractExit),
    run_process(
        Runner,
        [ PlanPath, '--dd-diet-rust-rust', '--watch-stdin' ],
        WatchOutput, RunnerOutput, RunnerExit),
    normalize_space(string(Json), RunnerOutput),
    atom_json_dict(Json, Result, [value_string_as(string)]),
    Traits = Result.deltas.source_trait.add,
    Observed = watch_result(
                   extract(ExtractExit),
                   runner(RunnerExit),
                   tick(Result.tick),
                   traits(Traits)).

run_process(Executable, Arguments, Input, Output, Exit) :-
    process_create(Executable, Arguments,
                   [ stdin(pipe(In)), stdout(pipe(Out)),
                     stderr(pipe(Err)), process(Pid)
                   ]),
    format(In, '~s', [Input]),
    close(In),
    read_string(Out, _, Output),
    close(Out),
    read_string(Err, _, _Error),
    close(Err),
    process_wait(Pid, Exit).

cleanup_watch_files(PlanPath, StatePath) :-
    catch(delete_file(PlanPath), _, true),
    catch(delete_file(StatePath), _, true).

:- end_tests(dl7_watch_pipeline).
