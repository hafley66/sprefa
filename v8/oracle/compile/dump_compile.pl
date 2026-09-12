% V7_DIR=<abs v7> swipl dump_compile.pl -- <out-dir> <stem> [--project <dir>] <path>...

:- use_module(library(json)).
:- prolog_load_context(directory, Here),
   assertz(here_dir(Here)),
   atomic_list_concat([Here, '/../eval/json_terms'], Helpers),
   consult(Helpers).
:- initialization(main, main).

v7_dir(V7) :-
    (   getenv('V7_DIR', V7)
    ->  true
    ;   here_dir(Here),
        atomic_list_concat([Here, '/../../../v7'], V7)
    ).

main([OutDir, Stem | Rest]) :-
    v7_dir(V7),
    atomic_list_concat([V7, '/src/2_comptime/2_compiler'], CompilerPath),
    use_module(CompilerPath,
               [compile_dl7/4, compile_dl7_project/5,
                compile_dl7_project_rows/6]),
    atomic_list_concat([V7, '/src/2_comptime/0c_extract_loader'], LoaderPath),
    use_module(LoaderPath, [load_tsi_stream/3]),
    run(Rest, Rows, Runtime, Diagnostics),
    term_json(Rows, RowsJson),
    term_json(Runtime, RuntimeJson),
    term_json(Diagnostics, DiagnosticsJson),
    atomic_list_concat([OutDir, '/', Stem, '.json'], OutPath),
    setup_call_cleanup(
        open(OutPath, write, Stream, [encoding(utf8)]),
        json_write_dict(Stream,
                        _{compiler_rows: RowsJson,
                          runtime_program: RuntimeJson,
                          diagnostics: DiagnosticsJson},
                        [width(0)]),
        close(Stream)).

run(['--project', Root | Paths0], Rows, Runtime, Diagnostics) :-
    !,
    partition_streams(Paths0, Sources, Streams),
    load_streams(Streams, TsiRows, StreamDiagnostics),
    (   StreamDiagnostics == []
    ->  compile_dl7_project_rows(Root, Sources, TsiRows,
                                 Rows, Runtime, Diagnostics)
    ;   Rows = [], Runtime = [], Diagnostics = StreamDiagnostics
    ).
run([Path], Rows, Runtime, Diagnostics) :-
    compile_dl7(Path, Rows, Runtime, Diagnostics).

partition_streams([], [], []).
partition_streams(['--tsi', Path | Rest], Sources, [Path | Streams]) :-
    !,
    partition_streams(Rest, Sources, Streams).
partition_streams([Path | Rest], [Path | Sources], Streams) :-
    partition_streams(Rest, Sources, Streams).

load_streams([], [], []).
load_streams([Path | Paths], Rows, Diagnostics) :-
    load_tsi_stream(Path, PathRows, PathDiagnostics),
    load_streams(Paths, RestRows, RestDiagnostics),
    append(PathRows, RestRows, Rows),
    append(PathDiagnostics, RestDiagnostics, Diagnostics).
