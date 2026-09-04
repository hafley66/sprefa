:- begin_tests(dl7_watch_pipeline).

:- use_module(library(http/json), [atom_json_dict/3, json_write_dict/3]).
:- use_module(library(process), [process_create/3, process_wait/2]).
:- use_module('../src/2_comptime/2_compiler', [compile_dl7_project_rows/6]).
:- use_module('../src/3_emit/1a_dbsp_plan_emitter', [emit_dbsp_plan/3]).
:- use_module('../src/4_tool/0_source_query_mainer', [extract_tsi_rows/4]).

test(checkout_snapshot_flows_through_the_resident_ram_program) :-
    watch_test(ram).

test(checkout_snapshot_flows_through_the_persistent_sqlite_program) :-
    watch_test(sqlite).

watch_test(Arm) :-
    tmp_file_stream(text, PlanPath, PlanStream),
    atom_concat(PlanPath, '.receipts.sqlite3', StatePath),
    atom_concat(PlanPath, '.runtime.sqlite3', RuntimePath),
    setup_call_cleanup(
        true,
        watch_pipeline(
            PlanPath, PlanStream, StatePath, RuntimePath, Arm, Observed),
        cleanup_watch_files(PlanPath, StatePath, RuntimePath)),
    expected_replay(Arm, ExpectedReplay),
    Observed == watch_result(
                    extract(exit(0)),
                    runner(exit(0)),
                    tick(0),
                    traits([["Render"]]),
                    replay(ExpectedReplay)).

watch_pipeline(PlanPath, PlanStream, StatePath, RuntimePath, Arm, Observed) :-
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
    runner_arguments(Arm, PlanPath, RuntimePath, RunnerArguments),
    run_process(
        Runner,
        RunnerArguments,
        WatchOutput, RunnerOutput, RunnerExit),
    normalize_space(string(Json), RunnerOutput),
    atom_json_dict(Json, Result, [value_string_as(string)]),
    Traits = Result.deltas.source_trait.add,
    replay_result(
        Arm, Runner, RunnerArguments, WatchOutput, Replay),
    Observed = watch_result(
                   extract(ExtractExit),
                   runner(RunnerExit),
                   tick(Result.tick),
                   traits(Traits),
                   replay(Replay)).

expected_replay(ram, skipped).
expected_replay(
    sqlite,
    persisted(
        duplicate(exit(0), add([]), del([])),
        removal(exit(0), add([]), del([["Render"]])))).

replay_result(ram, _, _, _, skipped).
replay_result(sqlite, Runner, RunnerArguments, WatchOutput,
              persisted(Duplicate, Removal)) :-
    watch_delta_stream(WatchOutput, 1, 1, DuplicateInput),
    replay_changes(
        Runner, RunnerArguments, DuplicateInput,
        duplicate(DuplicateExit, add(DuplicateAdd), del(DuplicateDel))),
    Duplicate = duplicate(
                    DuplicateExit, add(DuplicateAdd), del(DuplicateDel)),
    watch_delta_stream(WatchOutput, -1, 2, RemovalInput),
    replay_changes(
        Runner, RunnerArguments, RemovalInput,
        removal(RemovalExit, add(RemovalAdd), del(RemovalDel))),
    Removal = removal(RemovalExit, add(RemovalAdd), del(RemovalDel)).

replay_changes(Runner, RunnerArguments, Input,
               ResultTerm) :-
    run_process(
        Runner, RunnerArguments,
        Input, RunnerOutput, Exit),
    normalize_space(string(Json), RunnerOutput),
    atom_json_dict(Json, Result, [value_string_as(string)]),
    get_dict(deltas, Result, Deltas),
    ( get_dict(source_trait, Deltas, TraitDelta)
    -> Add = TraitDelta.add,
       Del = TraitDelta.del
    ;  Add = [],
       Del = []
    ),
    ResultTerm =.. [_, Exit, add(Add), del(Del)].

watch_delta_stream(Input, Sign, Generation, Output) :-
    split_string(Input, "\n", "\n", Lines),
    with_output_to(
        string(Output),
        maplist(write_delta_record(Sign, Generation), Lines)).

write_delta_record(Sign, Generation, Line) :-
    atom_json_dict(Line, Record0, [value_string_as(string)]),
    put_dict(generation, Record0, Generation, Record1),
    ( Record0.record == "batch_start"
    -> put_dict(mode, Record1, "delta", Record)
    ; Record0.record == "change"
    -> put_dict(sign, Record1, Sign, Record)
    ;  Record = Record1
    ),
    json_write_dict(current_output, Record, [width(0)]),
    nl.

runner_arguments(ram, PlanPath, _,
                 [PlanPath, '--dd-diet-rust-rust', '--watch-stdin']).
runner_arguments(sqlite, PlanPath, RuntimePath,
                 [PlanPath, '--sqlite-state', RuntimePath, '--watch-stdin']).

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

cleanup_watch_files(PlanPath, StatePath, RuntimePath) :-
    catch(delete_file(PlanPath), _, true),
    catch(delete_file(StatePath), _, true),
    catch(delete_file(RuntimePath), _, true).

:- end_tests(dl7_watch_pipeline).
