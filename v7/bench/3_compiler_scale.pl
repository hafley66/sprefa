% Deterministic DL7 compiler scaling workload.
%
% Usage:
%   swipl -q -s 3_compiler_scale.pl -g main -t halt -- \
%       <files> <types-per-file> <fields-per-type> <repetitions>
%
% Source generation, garbage collection, and compiler-cache clearing happen
% outside the measured interval. Each repetition measures one cold project
% compilation and emits one JSON object.

:- module(dl7_compiler_scale,
          [main/0]).

:- use_module(library(filesex),
              [ delete_directory_and_contents/1,
                make_directory_path/1
              ]).
:- use_module(library(http/json), [json_write_dict/3]).
:- use_module('../src/2_comptime/1c_compiler_cacher',
              [clear_compiler_caches/0]).
:- use_module('../src/2_comptime/2_compiler',
              [compile_dl7_project/5]).

main :-
    current_prolog_flag(argv, Arguments),
    (   scale_arguments(Arguments, Config)
    ->  run_scale(Config)
    ;   format(user_error,
               'usage: <files> <types-per-file> <fields-per-type> <repetitions>~n',
               []),
        halt(2)
    ).

scale_arguments([FilesAtom, TypesAtom, FieldsAtom, RepetitionsAtom | _],
                scale(Files, Types, Fields, Repetitions)) :-
    maplist(positive_integer,
            [FilesAtom, TypesAtom, FieldsAtom, RepetitionsAtom],
            [Files, Types, Fields, Repetitions]).

positive_integer(Atom, Integer) :-
    catch(atom_number(Atom, Integer), _, fail),
    integer(Integer),
    Integer > 0.

run_scale(scale(Files, Types, Fields, Repetitions)) :-
    v7_directory(V7Directory),
    scale_directory(V7Directory, ScaleDirectory),
    setup_call_cleanup(
        make_directory_path(ScaleDirectory),
        ( generate_project(ScaleDirectory, Files, Types, Fields,
                           Paths, SourceBytes),
          run_repetitions(Repetitions, V7Directory, Paths,
                          Files, Types, Fields, SourceBytes) ),
        delete_directory_and_contents(ScaleDirectory)).

v7_directory(V7Directory) :-
    source_file(dl7_compiler_scale:main, SourcePath),
    file_directory_name(SourcePath, BenchDirectory),
    directory_file_path(BenchDirectory, '..', V7Candidate),
    absolute_file_name(V7Candidate, V7Directory,
                       [file_type(directory), access(read)]).

scale_directory(V7Directory, ScaleDirectory) :-
    current_prolog_flag(pid, ProcessId),
    format(atom(Name), 'run-~d', [ProcessId]),
    directory_file_path(V7Directory, 'out/compiler-scale', Root),
    directory_file_path(Root, Name, ScaleDirectory).

generate_project(Directory, FileCount, TypeCount, FieldCount,
                 Paths, SourceBytes) :-
    FileCountMinusOne is FileCount - 1,
    numlist(0, FileCountMinusOne, FileIndices),
    maplist(generate_file(Directory, TypeCount, FieldCount),
            FileIndices, Paths, ByteCounts),
    sum_list(ByteCounts, SourceBytes).

generate_file(Directory, TypeCount, FieldCount, FileIndex,
              Path, SourceBytes) :-
    format(atom(FileName), '~d_scale_~d.dl7', [FileIndex, FileIndex]),
    directory_file_path(Directory, FileName, Path),
    setup_call_cleanup(
        open(Path, write, Stream, [encoding(utf8)]),
        write_scale_file(Stream, FileIndex, TypeCount, FieldCount),
        close(Stream)),
    size_file(Path, SourceBytes).

write_scale_file(Stream, FileIndex, TypeCount, FieldCount) :-
    FileTypeCountMinusOne is TypeCount - 1,
    numlist(0, FileTypeCountMinusOne, TypeIndices),
    maplist(write_scale_type(Stream, FileIndex, FieldCount), TypeIndices).

write_scale_type(Stream, FileIndex, FieldCount, TypeIndex) :-
    format(Stream, '(: ScaleF~dT~d~n   (*', [FileIndex, TypeIndex]),
    FieldCountMinusOne is FieldCount - 1,
    numlist(0, FieldCountMinusOne, FieldIndices),
    maplist(write_scale_field(Stream, FileIndex, TypeIndex), FieldIndices),
    format(Stream, '))~n', []).

write_scale_field(Stream, FileIndex, TypeIndex, FieldIndex) :-
    scale_field_target(FileIndex, TypeIndex, FieldIndex, Target),
    format(Stream, ' (: f~d ~w)', [FieldIndex, Target]).

scale_field_target(_, _, FieldIndex, int) :-
    0 is FieldIndex mod 4,
    !.
scale_field_target(_, _, FieldIndex, text) :-
    1 is FieldIndex mod 4,
    !.
scale_field_target(_, _, FieldIndex, '(Option text)') :-
    2 is FieldIndex mod 4,
    !.
scale_field_target(_, 0, _, int) :-
    !.
scale_field_target(FileIndex, TypeIndex, _, Target) :-
    PreviousTypeIndex is TypeIndex - 1,
    format(atom(Target), 'ScaleF~dT~d', [FileIndex, PreviousTypeIndex]).

run_repetitions(Repetitions, V7Directory, Paths,
                Files, Types, Fields, SourceBytes) :-
    numlist(1, Repetitions, Runs),
    maplist(run_repetition(V7Directory, Paths,
                          Files, Types, Fields, SourceBytes),
            Runs).

run_repetition(V7Directory, Paths, Files, Types, Fields, SourceBytes, Run) :-
    garbage_collect,
    clear_compiler_caches,
    statistics(inferences, BeforeInferences),
    get_time(BeforeWall),
    compile_dl7_project(V7Directory, Paths, Rows, Runtime, Diagnostics),
    get_time(AfterWall),
    statistics(inferences, AfterInferences),
    WallMs is round((AfterWall - BeforeWall) * 1000),
    Inferences is AfterInferences - BeforeInferences,
    length(Rows, RowCount),
    length(Diagnostics, DiagnosticCount),
    runtime_counts(Runtime, RuntimeRelations, RuntimeSeeds, RuntimeRules),
    TotalTypes is Files * Types,
    TotalFields is TotalTypes * Fields,
    Report = _{run: Run,
               files: Files,
               types_per_file: Types,
               fields_per_type: Fields,
               total_types: TotalTypes,
               total_fields: TotalFields,
               source_bytes: SourceBytes,
               wall_ms: WallMs,
               inferences: Inferences,
               compiler_rows: RowCount,
               diagnostics: DiagnosticCount,
               runtime_relations: RuntimeRelations,
               runtime_seeds: RuntimeSeeds,
               runtime_rules: RuntimeRules},
    json_write_dict(current_output, Report, [width(0)]),
    nl,
    report_diagnostics(Diagnostics).

report_diagnostics([]).
report_diagnostics(Diagnostics) :-
    Diagnostics = [_ | _],
    format(user_error, 'DL7-SCALE-DIAGNOSTICS ~q~n', [Diagnostics]),
    halt(1).

runtime_counts(
    checked_datalog(_, datalog_program(Relations, Seeds, Rules), _, _),
    RelationCount, SeedCount, RuleCount) :-
    !,
    length(Relations, RelationCount),
    length(Seeds, SeedCount),
    length(Rules, RuleCount).
runtime_counts(_, 0, 0, 0).
