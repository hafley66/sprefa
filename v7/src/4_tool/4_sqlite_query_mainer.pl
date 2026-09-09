:- module(dl7_sqlite_query_mainer,
          [ main/0,
            main/1,
            compile_sqlite_query/4,
            preflight_database/5,
            query_database/5,
            install_database/7,
            run_sqlite/6
          ]).

:- use_module(library(http/json),
              [atom_json_dict/3, json_write_dict/3]).
:- use_module(library(process), [process_create/3, process_wait/2]).
:- use_module(library(readutil), [read_file_to_string/3]).
:- use_module('../2_comptime/2_compiler', [compile_dl7/4]).
:- use_module('../3_emit/1c_sqlite_query_emitter',
              [ emit_sqlite_query/4,
                read_sqlite_layout/3,
                sqlite_identifier/2,
                sqlite_install_sql/3,
                sqlite_literal/2
              ]).

:- initialization(main, main).

main :-
    invoked_as_script,
    !,
    current_prolog_flag(argv, Arguments),
    main(Arguments).
main.

invoked_as_script :-
    module_property(dl7_sqlite_query_mainer, file(ThisFile)),
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
           'usage: dl7-sqlite-query PROGRAM LAYOUT DATABASE [--emit-sql|--query|--install NAME] [--sqlite3 BIN] [--ivm-extension LIB]~n',
           []).

parse_arguments([Program, Layout, Database | Options], Command) :-
    default_sqlite3(DefaultSqlite),
    default_extension(DefaultExtension),
    parse_options(
        Options,
        command(Program, Layout, Database, emit_sql,
                DefaultSqlite, DefaultExtension),
        Command).

parse_options([], Command, Command).
parse_options(['--emit-sql' | Options], Command0, Command) :-
    set_mode(emit_sql, Command0, Command1),
    parse_options(Options, Command1, Command).
parse_options(['--query' | Options], Command0, Command) :-
    set_mode(query, Command0, Command1),
    parse_options(Options, Command1, Command).
parse_options(['--install', Name | Options], Command0, Command) :-
    set_mode(install(Name), Command0, Command1),
    parse_options(Options, Command1, Command).
parse_options(['--sqlite3', Sqlite | Options],
              command(Program, Layout, Database, Mode, _, Extension),
              Command) :-
    parse_options(
        Options,
        command(Program, Layout, Database, Mode, Sqlite, Extension),
        Command).
parse_options(['--ivm-extension', Extension | Options],
              command(Program, Layout, Database, Mode, Sqlite, _),
              Command) :-
    parse_options(
        Options,
        command(Program, Layout, Database, Mode, Sqlite, Extension),
        Command).

set_mode(Mode,
         command(Program, Layout, Database, emit_sql, Sqlite, Extension),
         command(Program, Layout, Database, Mode, Sqlite, Extension)).

run_command(command(Program, Layout, Database, Mode, Sqlite, Extension),
            Code) :-
    compile_sqlite_query(Program, Layout, Artifact, Diagnostics),
    continue_command(
        Diagnostics, Mode, Sqlite, Extension, Database, Artifact, Code).

compile_sqlite_query(Program, LayoutPath, Artifact, Diagnostics) :-
    read_sqlite_layout(LayoutPath, LayoutRows, LayoutDiagnostics),
    compile_after_layout(
        LayoutDiagnostics, Program, LayoutRows, Artifact, Diagnostics).

compile_after_layout([], Program, LayoutRows, Artifact, Diagnostics) :-
    !,
    compile_dl7(Program, CompilerRows, RuntimeProgram, CompileDiagnostics),
    emit_after_compile(
        CompileDiagnostics, CompilerRows, RuntimeProgram,
        LayoutRows, Artifact, Diagnostics).
compile_after_layout(Diagnostics, _, _, _{}, Diagnostics).

emit_after_compile([], CompilerRows, RuntimeProgram,
                   LayoutRows, Artifact, Diagnostics) :-
    !,
    CompiledUnit = compiled_unit([], RuntimeProgram, CompilerRows),
    emit_sqlite_query(CompiledUnit, LayoutRows, Artifact, Diagnostics).
emit_after_compile(Diagnostics, _, _, _, _{}, Diagnostics).

continue_command([], emit_sql, _, _, _, Artifact, 0) :-
    !,
    json_write_dict(current_output, Artifact, [width(0)]),
    nl.
continue_command([], query, Sqlite, _, Database, Artifact, Code) :-
    !,
    query_database(Sqlite, Database, Artifact, Output, Diagnostics),
    sqlite_result(Diagnostics, Output, Code).
continue_command([], install(Name), Sqlite, Extension,
                 Database, Artifact, Code) :-
    !,
    install_database(
        Sqlite, Extension, Database, Name, Artifact, Output, Diagnostics),
    sqlite_result(Diagnostics, Output, Code).
continue_command(Diagnostics, _, _, _, _, _, 1) :-
    maplist(write_diagnostic, Diagnostics).

sqlite_result([], Output, 0) :-
    !,
    format('~s', [Output]).
sqlite_result(Diagnostics, _, 1) :-
    maplist(write_diagnostic, Diagnostics).

%% preflight_database(+Sqlite, +Database, +Artifact, +OutputName,
%%                    -Diagnostics) is det.
%
% OutputName is none for a read-only defining-query execution. An install name
% adds a case-insensitive sqlite_schema collision check.
preflight_database(Sqlite, Database, Artifact, OutputName, Diagnostics) :-
    preflight_sql(Artifact, OutputName, Sql),
    run_sqlite(Sqlite, Database, readonly, none, Sql, Result),
    validate_preflight_result(Result, Artifact, OutputName, Diagnostics).

preflight_sql(Artifact, OutputName, Sql) :-
    findall(Clause,
            ( member(Source, Artifact.sources),
              source_preflight_clauses(Source, SourceClauses),
              member(Clause, SourceClauses)
            ),
            SourceClauses),
    output_preflight_clauses(OutputName, OutputClauses),
    append(SourceClauses, OutputClauses, Clauses),
    atomics_to_string(Clauses, " UNION ALL ", Sql0),
    string_concat(Sql0, ";", Sql).

source_preflight_clauses(Source, [ObjectClause, ColumnClause]) :-
    sqlite_literal(Source.relation, Relation),
    sqlite_literal(Source.table, Table),
    format(string(ObjectClause),
           "SELECT 'object' AS check_kind, ~s AS relation_name, ~s AS binding_name, type AS value FROM main.sqlite_schema WHERE name = ~s COLLATE NOCASE",
           [Relation, Table, Table]),
    format(string(ColumnClause),
           "SELECT 'column' AS check_kind, ~s AS relation_name, ~s AS binding_name, name AS value FROM pragma_table_info(~s)",
           [Relation, Table, Table]).

output_preflight_clauses(none, []) :- !.
output_preflight_clauses(Name, [Clause]) :-
    sqlite_literal(Name, QuotedName),
    format(string(Clause),
           "SELECT 'output' AS check_kind, '' AS relation_name, ~s AS binding_name, type AS value FROM main.sqlite_schema WHERE name = ~s COLLATE NOCASE",
           [QuotedName, QuotedName]).

validate_preflight_result(
    sqlite_result(0, Output, _), Artifact, OutputName, Diagnostics) :-
    !,
    parse_sqlite_json(Output, Rows, ParseDiagnostics),
    validate_preflight_rows(
        ParseDiagnostics, Rows, Artifact, OutputName, Diagnostics).
validate_preflight_result(
    sqlite_result(Exit, _, Error), _, _,
    [diagnostic(sqlite, none, sqlite_process_exit(Exit, Error))]).

parse_sqlite_json(Output, Rows, Diagnostics) :-
    normalize_json_output(Output, Json),
    catch(atom_json_dict(Json, Rows, [value_string_as(string)]),
          Error,
          ParseError = Error),
    json_parse_result(ParseError, Rows, Diagnostics).

normalize_json_output(Output, "[]") :-
    normalize_space(string(""), Output),
    !.
normalize_json_output(Output, Output).

json_parse_result(ParseError, _,
                  [diagnostic(sqlite, none,
                              invalid_sqlite_json_output(ParseError))]) :-
    nonvar(ParseError),
    !.
json_parse_result(_, Rows, []) :-
    is_list(Rows),
    !.
json_parse_result(_, Rows,
                  [diagnostic(sqlite, none,
                              invalid_sqlite_json_rows(Rows))]).

validate_preflight_rows([Diagnostic | Diagnostics], _, _, _,
                        [Diagnostic | Diagnostics]) :-
    !.
validate_preflight_rows([], Rows, Artifact, OutputName, Diagnostics) :-
    findall(Reason,
            preflight_reason(Rows, Artifact, OutputName, Reason),
            Reasons0),
    sort(Reasons0, Reasons),
    maplist(sqlite_diagnostic, Reasons, Diagnostics).

preflight_reason(Rows, Artifact, _, Reason) :-
    member(Source, Artifact.sources),
    source_object_reason(Rows, Source, Reason).
preflight_reason(Rows, Artifact, _, Reason) :-
    member(Source, Artifact.sources),
    member(Column, Source.columns),
    \+ preflight_row(Rows, "column", Source.relation,
                     Source.table, Column),
    Reason = missing_sqlite_source_column(
                 Source.relation, Source.table, Column).
preflight_reason(Rows, _, OutputName, Reason) :-
    OutputName \== none,
    member(Row, Rows),
    Row.check_kind == "output",
    Reason = conflicting_sqlite_output(OutputName, Row.value).

source_object_reason(Rows, Source, Reason) :-
    findall(Type,
            preflight_row(Rows, "object", Source.relation,
                          Source.table, Type),
            Types),
    source_object_result(Source, Types, Reason).

source_object_result(_, ["table" | _], _) :-
    !,
    fail.
source_object_result(Source, [],
                     missing_sqlite_source_table(
                         Source.relation, Source.table)) :-
    !.
source_object_result(Source, Types,
                     sqlite_source_is_not_table(
                         Source.relation, Source.table, Types)).

preflight_row(Rows, Kind, Relation, Binding, Value) :-
    member(Row, Rows),
    Row.check_kind == Kind,
    Row.relation_name == Relation,
    Row.binding_name == Binding,
    Row.value = Value.

sqlite_diagnostic(Reason, diagnostic(sqlite, none, Reason)).

%% query_database(+Sqlite, +Database, +Artifact, -Output,
%%                -Diagnostics) is det.
query_database(Sqlite, Database, Artifact, Output, Diagnostics) :-
    preflight_database(Sqlite, Database, Artifact, none, PreflightDiagnostics),
    query_after_preflight(
        PreflightDiagnostics, Sqlite, Database, Artifact,
        Output, Diagnostics).

query_after_preflight([], Sqlite, Database, Artifact,
                      Output, Diagnostics) :-
    !,
    string_concat(Artifact.select_sql, ";", Sql),
    run_sqlite(Sqlite, Database, readonly, none, Sql, Result),
    sqlite_output_result(Result, Output, Diagnostics).
query_after_preflight(Diagnostics, _, _, _, "", Diagnostics).

%% install_database(+Sqlite, +Extension, +Database, +Name, +Artifact,
%%                  -Output, -Diagnostics) is det.
install_database(Sqlite, Extension, Database, Name, Artifact,
                 Output, Diagnostics) :-
    validate_install_inputs(Extension, Name, InputDiagnostics),
    preflight_after_install_inputs(
        InputDiagnostics, Sqlite, Extension, Database, Name, Artifact,
        Output, Diagnostics).

validate_install_inputs(Extension, Name, Diagnostics) :-
    catch(sqlite_identifier(Name, _),
          sqlite_query_error(Reason),
          NameError = Reason),
    install_input_diagnostics(NameError, Extension, Diagnostics).

install_input_diagnostics(NameError, _,
                          [diagnostic(sqlite, none, NameError)]) :-
    nonvar(NameError),
    !.
install_input_diagnostics(_, Extension, []) :-
    exists_file(Extension),
    !.
install_input_diagnostics(_, Extension,
                          [diagnostic(sqlite, none,
                                      missing_ivm_extension(Extension))]).

preflight_after_install_inputs(
    [], Sqlite, Extension, Database, Name, Artifact,
    Output, Diagnostics) :-
    !,
    preflight_database(
        Sqlite, Database, Artifact, Name, PreflightDiagnostics),
    install_after_preflight(
        PreflightDiagnostics, Sqlite, Extension, Database,
        Name, Artifact, Output, Diagnostics).
preflight_after_install_inputs(
    Diagnostics, _, _, _, _, _, "", Diagnostics).

install_after_preflight([], Sqlite, Extension, Database,
                        Name, Artifact, Output, Diagnostics) :-
    !,
    sqlite_install_sql(Name, Artifact.select_sql, CreateSql),
    sqlite_identifier(Name, QuotedName),
    format(string(Sql),
           "PRAGMA recursive_triggers=ON; PRAGMA trusted_schema=ON; BEGIN IMMEDIATE; ~s COMMIT; SELECT * FROM ~s;",
           [CreateSql, QuotedName]),
    run_sqlite(Sqlite, Database, readwrite, Extension, Sql, Result),
    sqlite_output_result(Result, Output, Diagnostics).
install_after_preflight(Diagnostics, _, _, _, _, _, "", Diagnostics).

sqlite_output_result(sqlite_result(0, Output, _), Output, []) :- !.
sqlite_output_result(sqlite_result(Exit, _, Error), "",
                     [diagnostic(sqlite, none,
                                 sqlite_process_exit(Exit, Error))]).

%% run_sqlite(+Sqlite, +Database, +Access, +Extension, +Sql, -Result) is det.
run_sqlite(Sqlite0, Database, Access, Extension, Sql,
           sqlite_result(Exit, Output, Error)) :-
    executable_spec(Sqlite0, Executable),
    sqlite_arguments(Access, Extension, Database, Sql, Arguments),
    setup_call_cleanup(
        tmp_file_stream(text, ErrorPath, ErrorStream),
        run_sqlite_process(
            Executable, Arguments, ErrorStream,
            Exit, Output, ErrorPath, Error),
        cleanup_error_file(ErrorStream, ErrorPath)).

run_sqlite_process(Executable, Arguments, ErrorStream,
                   Exit, Output, ErrorPath, Error) :-
    process_create(Executable, Arguments,
                   [ stdout(pipe(Out)),
                     stderr(stream(ErrorStream)),
                     process(Pid)
                   ]),
    read_string(Out, _, Output),
    close(Out),
    process_wait(Pid, Status),
    flush_output(ErrorStream),
    read_file_to_string(ErrorPath, Error, [encoding(utf8)]),
    process_exit_code(Status, Exit).

cleanup_error_file(ErrorStream, ErrorPath) :-
    (   is_stream(ErrorStream)
    ->  close(ErrorStream, [force(true)])
    ;   true
    ),
    (   exists_file(ErrorPath)
    ->  delete_file(ErrorPath)
    ;   true
    ).

sqlite_arguments(Access, Extension, Database, Sql, Arguments) :-
    access_arguments(Access, AccessArguments),
    extension_arguments(Extension, ExtensionArguments),
    connection_sql(Extension, Sql, ConnectionSql),
    append([
        ['-batch', '-bail', '-json'],
        AccessArguments,
        ExtensionArguments,
        [Database, ConnectionSql]
    ], Arguments).

connection_sql(none, Sql, Sql) :- !.
connection_sql(_, Sql, ConnectionSql) :-
    format(string(ConnectionSql),
           "PRAGMA recursive_triggers=ON; PRAGMA trusted_schema=ON; ~s",
           [Sql]).

access_arguments(readonly, ['-readonly']) :- !.
access_arguments(readwrite, []).

extension_arguments(none, []) :- !.
extension_arguments(Extension, ['-cmd', Command]) :-
    sqlite_dot_argument(Extension, QuotedExtension),
    format(string(Command), ".load ~s", [QuotedExtension]).

sqlite_dot_argument(Value0, Quoted) :-
    text_string(Value0, Value),
    \+ sub_string(Value, _, _, _, "\u0000"),
    \+ sub_string(Value, _, _, _, "\n"),
    \+ sub_string(Value, _, _, _, "\r"),
    string_codes(Value, Codes),
    dot_escaped_codes(Codes, EscapedCodes),
    string_codes(Escaped, EscapedCodes),
    format(string(Quoted), "\"~s\"", [Escaped]).

dot_escaped_codes([], []).
dot_escaped_codes([0'\\ | Codes], [0'\\, 0'\\ | Escaped]) :-
    !,
    dot_escaped_codes(Codes, Escaped).
dot_escaped_codes([0'\" | Codes], [0'\\, 0'\" | Escaped]) :-
    !,
    dot_escaped_codes(Codes, Escaped).
dot_escaped_codes([Code | Codes], [Code | Escaped]) :-
    dot_escaped_codes(Codes, Escaped).

executable_spec(Sqlite0, Executable) :-
    text_string(Sqlite0, Sqlite),
    atom_string(SqliteAtom, Sqlite),
    (   sub_string(Sqlite, _, _, _, "/")
    ->  Executable = SqliteAtom
    ;   Executable = path(SqliteAtom)
    ).

process_exit_code(exit(Code), Code) :- !.
process_exit_code(killed(Signal), killed(Signal)) :- !.
process_exit_code(Status, Status).

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
