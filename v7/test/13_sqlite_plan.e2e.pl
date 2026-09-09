:- begin_tests(dl7_sqlite_plan).

:- use_module(library(http/json), [atom_json_dict/3, json_write_dict/3]).
:- use_module(library(process), [process_create/3, process_wait/2]).
:- use_module('../src/2_comptime/2_compiler', [compile_dl7/4]).
:- use_module('../src/3_emit/1a_dbsp_plan_emitter', [emit_dbsp_plan/3]).

test(generated_plan_executes_the_same_tick_in_sqlite_and_ram) :-
    compile_dl7('v7/test/fixtures/12_native_runtime.dl7',
                _, Runtime, CompileDiagnostics),
    emit_dbsp_plan(Runtime, Plan0, EmitDiagnostics),
    put_dict(
        _{ initial:[],
           schedule:[[_{sign:1, rel:'Input', values:[7, 7, 7]}]]
         },
        Plan0, Plan),
    setup_call_cleanup(
        tmp_file_stream(text, PlanPath, PlanStream),
        execute_both_arms(PlanPath, PlanStream, Plan, Sqlite, Ram),
        catch(delete_file(PlanPath), _, true)),
    Expected = tick(
                   1,
                   #{ 'Input': #{add:[[7, 7, 7]], del:[]},
                      'Output': #{add:[[7]], del:[]}
                    }),
    CompileDiagnostics == [],
    EmitDiagnostics == [],
    Sqlite =@= Expected,
    Ram =@= Expected.

execute_both_arms(PlanPath, PlanStream, Plan, Sqlite, Ram) :-
    json_write_dict(PlanStream, Plan, [width(0)]),
    nl(PlanStream),
    close(PlanStream),
    run_runner(PlanPath, [], Sqlite),
    run_runner(PlanPath, ['--dd-diet-rust-rust'], Ram).

run_runner(PlanPath, Options, tick(Tick, Deltas)) :-
    append([PlanPath], Options, Arguments),
    process_create('v6/dd-runner/target/debug/dd-runner', Arguments,
                   [ stdout(pipe(Out)), stderr(pipe(Err)), process(Pid) ]),
    read_string(Out, _, Output),
    close(Out),
    read_string(Err, _, Error),
    close(Err),
    process_wait(Pid, Exit),
    Exit == exit(0),
    Error == "",
    normalize_space(string(Json), Output),
    atom_json_dict(Json, Result, [value_string_as(string)]),
    Tick = Result.tick,
    Deltas = Result.deltas.

:- end_tests(dl7_sqlite_plan).
