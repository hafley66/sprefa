:- module(dl7_extract_sqlite_query_mainer,
          [ main/0,
            main/1,
            extract_sqlite_query/7,
            run_extract_process/4
          ]).

:- use_module(library(process), [process_create/3, process_wait/2]).
:- use_module(library(readutil), [read_file_to_string/3]).
:- use_module('4_sqlite_query_mainer',
              [ compile_sqlite_query/4,
                install_database/7,
                query_database/5
              ]).
:- use_module('../3_emit/1c_sqlite_query_emitter',
              [sqlite_identifier/2]).

:- initialization(main, main).

main :-
    invoked_as_script,
    !,
    current_prolog_flag(argv, Arguments),
    main(Arguments).
main.

invoked_as_script :-
    module_property(dl7_extract_sqlite_query_mainer, file(ThisFile)),
    current_prolog_flag(os_argv, ProcessArguments),
    member(Argument, ProcessArguments),
    catch(same_file(Argument, ThisFile), _, fail),
    !.

main(Arguments) :-
    catch(driver_exit_code(Arguments, Code),
          Error,
          ( write_term(user_error, implementation_failure(Error),
                       [quoted(true), fullstop(true), nl(true)]),
            Code = 1
          )),
    halt(Code).

driver_exit_code(Arguments, Code) :-
    parse_arguments(Arguments, Command),
    !,
    run_command(Command, Code).
driver_exit_code(_, 2) :-
    format(user_error,
           'usage: dl7-extract-sqlite-query PROGRAM LAYOUT DATABASE [--extract BIN] [--install NAME] [--sqlite3 BIN] [--ivm-extension LIB] -- SOURCE...~n',
           []).

parse_arguments(Arguments, Command) :-
    append(Prefix, ['--' | Sources], Arguments),
    Prefix = [Program, Layout, Database | Options],
    Sources = [_ | _],
    maplist(nonempty_argument, Sources),
    default_extract(DefaultExtract),
    default_sqlite3(DefaultSqlite),
    default_extension(DefaultExtension),
    parse_options(
        Options,
        command(Program, Layout, Database, Sources,
                query, DefaultExtract, DefaultSqlite, DefaultExtension),
        Command),
    validate_command(Command),
    !.

parse_options([], Command, Command).
parse_options(['--extract', Extract | Options],
              command(Program, Layout, Database, Sources,
                      Mode, _, Sqlite, Extension),
              Command) :-
    nonempty_argument(Extract),
    parse_options(
        Options,
        command(Program, Layout, Database, Sources,
                Mode, Extract, Sqlite, Extension),
        Command).
parse_options(['--install', Name | Options], Command0, Command) :-
    nonempty_argument(Name),
    set_install_mode(Name, Command0, Command1),
    parse_options(Options, Command1, Command).
parse_options(['--sqlite3', Sqlite | Options],
              command(Program, Layout, Database, Sources,
                      Mode, Extract, _, Extension),
              Command) :-
    nonempty_argument(Sqlite),
    parse_options(
        Options,
        command(Program, Layout, Database, Sources,
                Mode, Extract, Sqlite, Extension),
        Command).
parse_options(['--ivm-extension', Extension | Options],
              command(Program, Layout, Database, Sources,
                      Mode, Extract, Sqlite, _),
              Command) :-
    nonempty_argument(Extension),
    parse_options(
        Options,
        command(Program, Layout, Database, Sources,
                Mode, Extract, Sqlite, Extension),
        Command).

set_install_mode(
    Name,
    command(Program, Layout, Database, Sources,
            query, Extract, Sqlite, Extension),
    command(Program, Layout, Database, Sources,
            install(Name), Extract, Sqlite, Extension)).

validate_command(
    command(Program, Layout, Database, Sources,
            Mode, Extract, Sqlite, Extension)) :-
    maplist(nonempty_argument,
            [Program, Layout, Database, Extract, Sqlite]),
    Sources = [_ | _],
    validate_mode(Mode, Extension).

validate_mode(query, _).
validate_mode(install(Name), Extension) :-
    nonempty_argument(Extension),
    catch(sqlite_identifier(Name, _), _, fail).

nonempty_argument(Value) :-
    text_string(Value, Text),
    Text \== "".

run_command(
    command(Program, Layout, Database, Sources,
            Mode, Extract, Sqlite, Extension),
    Code) :-
    Options = extract_options(Mode, Extract, Sqlite, Extension),
    extract_sqlite_query(
        Program, Layout, Database, Sources, Options,
        Output, Diagnostics),
    command_result(Diagnostics, Output, Code).

command_result([], Output, 0) :-
    !,
    format('~s', [Output]).
command_result(Diagnostics, _, 1) :-
    maplist(write_diagnostic, Diagnostics).

%% extract_sqlite_query(+Program, +Layout, +Database, +Sources, +Options,
%%                      -Output, -Diagnostics) is det.
%
% Options is extract_options(Mode, Extract, Sqlite, Extension), where Mode is
% query or install(Name). Program and target layout validation complete before
% the destination path is inspected or the extractor is started.
extract_sqlite_query(
    Program, Layout, Database, Sources,
    extract_options(Mode, Extract, Sqlite, Extension),
    Output, Diagnostics) :-
    compile_sqlite_query(Program, Layout, Artifact, CompileDiagnostics),
    continue_after_compile(
        CompileDiagnostics, Mode, Extract, Sqlite, Extension,
        Database, Sources, Artifact, Output, Diagnostics).

continue_after_compile(
    [], Mode, Extract, Sqlite, Extension,
    Database, Sources, Artifact, Output, Diagnostics) :-
    !,
    (   exists_file(Database)
    ->  Output = "",
        Diagnostics = [diagnostic(
                           extract, file(Database),
                           sqlite_destination_exists)]
    ;   run_extract_process(
            Extract, Database, Sources, ExtractResult),
        forward_extract_streams(ExtractResult),
        continue_after_extract(
            ExtractResult, Mode, Sqlite, Extension,
            Database, Artifact, Output, Diagnostics)
    ).
continue_after_compile(
    Diagnostics, _, _, _, _, _, _, _, "", Diagnostics).

continue_after_extract(
    extract_result(0, _, _), query, Sqlite, _,
    Database, Artifact, Output, Diagnostics) :-
    !,
    query_database(Sqlite, Database, Artifact,
                   QueryOutput, QueryDiagnostics),
    post_extract_result(
        QueryDiagnostics, Database, QueryOutput, Output, Diagnostics).
continue_after_extract(
    extract_result(0, _, _), install(Name), Sqlite, Extension,
    Database, Artifact, Output, Diagnostics) :-
    !,
    install_database(
        Sqlite, Extension, Database, Name, Artifact,
        InstallOutput, InstallDiagnostics),
    post_extract_result(
        InstallDiagnostics, Database, InstallOutput, Output, Diagnostics).
continue_after_extract(
    extract_result(process_error(Error), _, _), _, _, _, _, _, "",
    [diagnostic(extract, none, process_error(Error))]) :-
    !.
continue_after_extract(
    extract_result(Exit, _, _), _, _, _, _, _, "",
    [diagnostic(extract, none, process_exit(Exit))]).

post_extract_result([], _, Output, Output, []) :- !.
post_extract_result(QueryDiagnostics, Database, _, "", Diagnostics) :-
    append(QueryDiagnostics,
           [diagnostic(extract, file(Database),
                       sqlite_database_retained_after_query_failure)],
           Diagnostics).

%% run_extract_process(+Extract, +Database, +Sources, -Result) is det.
run_extract_process(Extract, Database, Sources, Result) :-
    ExtractArguments = ['--sqlite', Database, '--' | Sources],
    catch(
        run_spooled_process(Extract, ExtractArguments, Result),
        Error,
        Result = extract_result(process_error(Error), "", "")).

run_spooled_process(Executable0, Arguments,
                    extract_result(Exit, Output, Error)) :-
    executable_spec(Executable0, Executable),
    setup_call_cleanup(
        spools(OutPath, OutStream, ErrPath, ErrStream),
        run_process_to_spools(
            Executable, Arguments,
            OutPath, OutStream, ErrPath, ErrStream,
            Exit, Output, Error),
        cleanup_spools(OutPath, OutStream, ErrPath, ErrStream)).

spools(OutPath, OutStream, ErrPath, ErrStream) :-
    tmp_file_stream(text, OutPath, OutStream),
    tmp_file_stream(text, ErrPath, ErrStream).

run_process_to_spools(
    Executable, Arguments,
    OutPath, OutStream, ErrPath, ErrStream,
    Exit, Output, Error) :-
    process_create(Executable, Arguments,
                   [ stdout(stream(OutStream)),
                     stderr(stream(ErrStream)),
                     process(Pid)
                   ]),
    process_wait(Pid, Status),
    flush_output(OutStream),
    flush_output(ErrStream),
    read_file_to_string(OutPath, Output, [encoding(utf8)]),
    read_file_to_string(ErrPath, Error, [encoding(utf8)]),
    process_exit_code(Status, Exit).

cleanup_spools(OutPath, OutStream, ErrPath, ErrStream) :-
    close_if_stream(OutStream),
    close_if_stream(ErrStream),
    delete_if_file(OutPath),
    delete_if_file(ErrPath).

close_if_stream(Stream) :-
    (   is_stream(Stream)
    ->  close(Stream, [force(true)])
    ;   true
    ).

delete_if_file(Path) :-
    (   exists_file(Path)
    ->  delete_file(Path)
    ;   true
    ).

forward_extract_streams(extract_result(_, Output, Error)) :-
    forward_extract_text(Output),
    forward_extract_text(Error).

forward_extract_text("") :- !.
forward_extract_text(Text) :-
    format(user_error, '~s', [Text]).

executable_spec(Executable0, Executable) :-
    text_string(Executable0, ExecutableText),
    atom_string(ExecutableAtom, ExecutableText),
    (   sub_string(ExecutableText, _, _, _, "/")
    ->  Executable = ExecutableAtom
    ;   Executable = path(ExecutableAtom)
    ).

process_exit_code(exit(Code), Code) :- !.
process_exit_code(killed(Signal), killed(Signal)) :- !.
process_exit_code(Status, Status).

default_extract(Extract) :-
    (   getenv('EXTRACT', EnvironmentExtract),
        EnvironmentExtract \== ''
    ->  atom_string(EnvironmentExtract, Extract)
    ;   Extract = "extract"
    ).

default_sqlite3(Sqlite) :-
    (   getenv('SQLITE3', EnvironmentSqlite),
        EnvironmentSqlite \== ''
    ->  atom_string(EnvironmentSqlite, Sqlite)
    ;   exists_file('/opt/homebrew/opt/sqlite/bin/sqlite3')
    ->  Sqlite = "/opt/homebrew/opt/sqlite/bin/sqlite3"
    ;   Sqlite = "sqlite3"
    ).

default_extension(Extension) :-
    (   getenv('IVM_EXTENSION', EnvironmentExtension),
        EnvironmentExtension \== ''
    ->  atom_string(EnvironmentExtension, Extension)
    ;   local_extension(Extension)
    ->  true
    ;   Extension = ""
    ).

local_extension(Extension) :-
    source_file(main, ThisFile),
    file_directory_name(ThisFile, ToolDirectory),
    member(Relative,
           [ '../../../sqlite_ivm/target/release/libsqlite_ivm.dylib',
             '../../../sqlite_ivm/target/release/libsqlite_ivm.so'
           ]),
    directory_file_path(ToolDirectory, Relative, Candidate),
    absolute_file_name(Candidate, Absolute,
                       [access(read), file_errors(fail)]),
    atom_string(Absolute, Extension),
    !.

write_diagnostic(Diagnostic) :-
    write_term(user_error, Diagnostic,
               [ quoted(true), ignore_ops(true), numbervars(true),
                 fullstop(true), nl(true)
               ]).

text_string(Value, Text) :-
    string(Value),
    !,
    Text = Value.
text_string(Value, Text) :-
    atom(Value),
    atom_string(Value, Text).
