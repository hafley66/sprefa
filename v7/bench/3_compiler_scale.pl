% Deterministic DL7 compiler scaling workload.
%
% Usage:
%   swipl -q -s 3_compiler_scale.pl -g main -t halt -- \
%       <files> <types-per-file> <fields-per-type> <repetitions> [off|collect]
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
:- use_module(library(pairs), [pairs_keys_values/3]).
:- use_module('../src/2_comptime/1c_compiler_cacher',
              [clear_compiler_caches/0]).
:- use_module('../src/2_comptime/1b_compiler_tracer',
              [latest_compile_trace/4]).
:- use_module('../src/2_comptime/2_compiler',
              [compile_dl7_project/5]).

:- meta_predicate with_trace_mode(+, 0).

main :-
    current_prolog_flag(argv, Arguments),
    (   scale_arguments(Arguments, Config)
    ->  run_scale(Config)
    ;   format(user_error,
               'usage: <files> <types-per-file> <fields-per-type> <repetitions> [off|collect]~n',
               []),
        halt(2)
    ).

scale_arguments([FilesAtom, TypesAtom, FieldsAtom, RepetitionsAtom | Rest],
                scale(Files, Types, Fields, Repetitions, TraceMode)) :-
    maplist(positive_integer,
            [FilesAtom, TypesAtom, FieldsAtom, RepetitionsAtom],
            [Files, Types, Fields, Repetitions]),
    trace_argument(Rest, TraceMode).

trace_argument([], off).
trace_argument([TraceMode | _], TraceMode) :-
    memberchk(TraceMode, [off, collect]).

positive_integer(Atom, Integer) :-
    catch(atom_number(Atom, Integer), _, fail),
    integer(Integer),
    Integer > 0.

run_scale(scale(Files, Types, Fields, Repetitions, TraceMode)) :-
    v7_directory(V7Directory),
    scale_directory(V7Directory, ScaleDirectory),
    setup_call_cleanup(
        make_directory_path(ScaleDirectory),
        ( generate_project(ScaleDirectory, Files, Types, Fields,
                           Paths, SourceBytes),
          with_trace_mode(
              TraceMode,
              run_repetitions(Repetitions, V7Directory, Paths,
                              Files, Types, Fields, SourceBytes, TraceMode)) ),
        delete_directory_and_contents(ScaleDirectory)).

with_trace_mode(TraceMode, Goal) :-
    prior_trace_mode(Prior),
    setup_call_cleanup(
        apply_trace_mode(TraceMode),
        call(Goal),
        restore_trace_mode(Prior)).

prior_trace_mode(value(Value)) :-
    getenv('DL7_TRACE', Value),
    !.
prior_trace_mode(unset).

apply_trace_mode(off) :-
    unsetenv('DL7_TRACE').
apply_trace_mode(collect) :-
    setenv('DL7_TRACE', collect).

restore_trace_mode(unset) :-
    unsetenv('DL7_TRACE').
restore_trace_mode(value(Value)) :-
    setenv('DL7_TRACE', Value).

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
                Files, Types, Fields, SourceBytes, TraceMode) :-
    numlist(1, Repetitions, Runs),
    maplist(run_repetition(V7Directory, Paths,
                          Files, Types, Fields, SourceBytes, TraceMode),
            Runs).

run_repetition(V7Directory, Paths, Files, Types, Fields, SourceBytes,
               TraceMode, Run) :-
    garbage_collect,
    clear_compiler_caches,
    statistics(inferences, BeforeInferences),
    get_time(BeforeWall),
    compile_dl7_project(V7Directory, Paths, Rows, Runtime, Diagnostics),
    get_time(AfterWall),
    statistics(inferences, AfterInferences),
    latest_compile_trace(_, Phases, Steps, _),
    WallMs is round((AfterWall - BeforeWall) * 1000),
    Inferences is AfterInferences - BeforeInferences,
    length(Rows, RowCount),
    length(Diagnostics, DiagnosticCount),
    runtime_counts(Runtime, RuntimeRelations, RuntimeSeeds, RuntimeRules),
    phase_totals(Phases, PhaseTotals),
    step_totals(Steps, StepTotals),
    TotalTypes is Files * Types,
    TotalFields is TotalTypes * Fields,
    Report = _{run: Run,
               trace_mode: TraceMode,
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
               runtime_rules: RuntimeRules,
               phases: PhaseTotals,
               steps: StepTotals},
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

phase_totals(Phases, Totals) :-
    findall(Name, member(phase(Name, _), Phases), Names0),
    sort(Names0, Names),
    maplist(phase_total(Phases), Names, Totals).

phase_total(Phases, Name,
            _{phase: Name, wall_ms: WallMs, inferences: Inferences}) :-
    findall(Wall-Inference,
            ( member(phase(Name, Measurement), Phases),
              measurement_wall_inferences(Measurement, Wall, Inference) ),
            Measurements),
    pairs_keys_values(Measurements, Walls, InferenceCounts),
    sum_list(Walls, WallMs),
    sum_list(InferenceCounts, Inferences).

measurement_wall_inferences(
    measurement(WallMs, _, Inferences, _, _, _, _, _, _, _, _, _),
    WallMs, Inferences).

step_totals(Steps, Totals) :-
    findall(Phase-Name,
            ( member(step(_, Phase, Step, _, _), Steps),
              step_name(Step, Name) ),
            Keys0),
    sort(Keys0, Keys),
    maplist(step_total(Steps), Keys, Totals).

step_total(Steps, Phase-Name,
           _{phase: Phase, step: Name,
             wall_ms: WallMs, inferences: Inferences}) :-
    findall(Wall-Inference,
            ( member(step(_, Phase, Step, Measurement, _), Steps),
              step_name(Step, Name),
              measurement_wall_inferences(Measurement, Wall, Inference) ),
            Measurements),
    pairs_keys_values(Measurements, Walls, InferenceCounts),
    sum_list(Walls, WallMs),
    sum_list(InferenceCounts, Inferences).

step_name(Name, Name) :-
    atom(Name),
    !.
step_name(Term, Name) :-
    format(atom(Name), '~w', [Term]).
