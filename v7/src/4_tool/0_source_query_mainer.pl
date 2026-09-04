:- module(dl7_source_query_mainer,
          [ extract_tsi_rows/4,
            query_source/6,
            query_tsi_rows/5,
            main/0,
            main/1
          ]).

:- use_module(library(http/json), [json_write/3]).
:- use_module(library(process), [process_create/3, process_wait/2]).
:- use_module('../2_comptime/0c_extract_loader', [load_tsi_text/3]).
:- use_module('../2_comptime/2_compiler', [compile_dl7_project_rows/6]).

:- initialization(main, main).

%% query_source(+Extract, +ProgramPath, +SourcePath, +RelationName,
%%              -Rows, -Diagnostics) is det.
%
% Extract one source file's type facts, compile the DL7 program against them,
% then return the selected program-owned relation tuples.
query_source(Extract, ProgramPath, SourcePath, RelationName,
             Rows, Diagnostics) :-
    extract_tsi_rows(Extract, SourcePath, TsiRows, ExtractDiagnostics),
    continue_after_load(
        ExtractDiagnostics, ProgramPath, RelationName, TsiRows,
        Rows, Diagnostics).

%% extract_tsi_rows(+Extract, +SourcePath, -Rows, -Diagnostics) is det.
extract_tsi_rows(Extract, SourcePath, TsiRows, Diagnostics) :-
    once(absolute_file_name(SourcePath, AbsoluteSource,
                            [access(read), file_errors(error)])),
    run_process(Extract,
                ['--witness', '--family', type, AbsoluteSource],
                Exit, StreamText, Error),
    continue_after_extract(Exit, Error, StreamText, TsiRows, Diagnostics).

continue_after_extract(0, _, StreamText, Rows, Diagnostics) :-
    !,
    load_tsi_text(StreamText, Rows, Diagnostics).
continue_after_extract(Exit, Error, _, [],
                       [diagnostic(extract, none,
                                   process_exit(Exit, Error))]).

continue_after_load([], ProgramPath, RelationName, TsiRows,
                    Rows, Diagnostics) :-
    !,
    query_tsi_rows(ProgramPath, TsiRows, RelationName, Rows, Diagnostics).
continue_after_load(Diagnostics, _, _, _, [], Diagnostics).

%% query_tsi_rows(+ProgramPath, +TsiRows, +RelationName,
%%                -Rows, -Diagnostics) is det.
query_tsi_rows(ProgramPath, TsiRows, RelationName, Rows, Diagnostics) :-
    once(absolute_file_name(ProgramPath, AbsoluteProgram,
                            [access(read), file_errors(error)])),
    file_directory_name(AbsoluteProgram, Root),
    compile_dl7_project_rows(
        Root, [AbsoluteProgram], TsiRows,
        CompilerRows, Runtime, CompileDiagnostics),
    select_query_rows(
        CompileDiagnostics, AbsoluteProgram, RelationName,
        CompilerRows, Runtime, Rows, Diagnostics).

select_query_rows([], ProgramPath, RelationName, CompilerRows,
                  checked_datalog(root_graph(_, Edges), _, _, _),
                  Rows, Diagnostics) :-
    !,
    ProgramOwner = module(file(ProgramPath)),
    (   memberchk(':'(ProgramOwner, RelationName, ref(Relation), _), Edges)
    ->  findall(Arguments,
                member(call(ref(Relation), Arguments), CompilerRows),
                Rows0),
        sort(Rows0, Rows),
        Diagnostics = []
    ;   Rows = [],
        Diagnostics = [diagnostic(query, file(ProgramPath),
                                  unknown_relation(RelationName))]
    ).
select_query_rows(Diagnostics, _, _, _, _, [], Diagnostics).

run_process(Executable, Arguments, Exit, Output, Error) :-
    process_create(Executable, Arguments,
                   [ stdout(pipe(Out)),
                     stderr(pipe(Err)),
                     process(Pid)
                   ]),
    read_string(Out, _, Output),
    close(Out),
    read_string(Err, _, Error),
    close(Err),
    process_wait(Pid, Status),
    process_exit_code(Status, Exit).

process_exit_code(exit(Code), Code) :- !.
process_exit_code(killed(Signal), killed(Signal)) :- !.
process_exit_code(Status, Status).

main :-
    current_prolog_flag(argv, Arguments),
    main(Arguments).

main(Arguments) :-
    catch(driver_exit_code(Arguments, Code),
          Error,
          implementation_failure(Error, Code)),
    halt(Code).

driver_exit_code(Arguments, Code) :-
    parse_arguments(Arguments, Program, Source, Relation, Extract),
    !,
    query_source(Extract, Program, Source, Relation, Rows, Diagnostics),
    query_exit(Diagnostics, Rows, Code).
driver_exit_code(_, 2) :-
    format(user_error,
           'usage: dl7-query PROGRAM SOURCE RELATION [--extract BIN]~n', []).

parse_arguments([Program, Source, Relation | Options],
                Program, Source, Relation, Extract) :-
    default_extract(DefaultExtract),
    parse_options(Options, DefaultExtract, Extract).

parse_options([], Extract, Extract).
parse_options(['--extract', Binary], _, Binary).

default_extract(Extract) :-
    (   getenv('SPREFA_EXTRACT_BIN', Extract)
    ->  true
    ;   source_file(main, ThisFile),
        file_directory_name(ThisFile, ToolDirectory),
        directory_file_path(
            ToolDirectory,
            '../../../v6/sprefa-extract/target/debug/extract',
            RelativeExtract),
        absolute_file_name(RelativeExtract, Extract,
                           [access(execute), file_errors(error)])
    ).

query_exit([], Rows, 0) :-
    !,
    maplist(write_json_row, Rows).
query_exit(Diagnostics, _, 1) :-
    maplist(write_diagnostic, Diagnostics).

write_json_row(Arguments) :-
    maplist(argument_json, Arguments, Json),
    json_write(current_output, Json, [width(0)]),
    nl.

argument_json(const(Value), Json) :-
    !,
    constant_json(Value, Json).
argument_json(ref(Identity), _{ref:Text}) :-
    !,
    term_string(Identity, Text, [quoted(true), numbervars(true)]).
argument_json(Term, _{term:Text}) :-
    term_string(Term, Text, [quoted(true), numbervars(true)]).

constant_json(Value, Value) :-
    ( integer(Value)
    ; float(Value)
    ; string(Value)
    ),
    !.
constant_json(Value, Text) :-
    atom(Value),
    !,
    atom_string(Value, Text).
constant_json(span(Digest, Start, End),
              _{span:_{digest:DigestText, start:Start, end:End}}) :-
    !,
    atom_string(Digest, DigestText).
constant_json(Values, Json) :-
    is_list(Values),
    !,
    maplist(constant_json, Values, Json).
constant_json(Value, _{term:Text}) :-
    term_string(Value, Text, [quoted(true), numbervars(true)]).

write_diagnostic(Diagnostic) :-
    write_term(user_error, Diagnostic,
               [ quoted(true), ignore_ops(true), numbervars(true),
                 fullstop(true), nl(true)
               ]).

implementation_failure(Error, 1) :-
    write_term(user_error, implementation_failure(Error),
               [ quoted(true), ignore_ops(true), numbervars(true),
                 fullstop(true), nl(true)
               ]).
