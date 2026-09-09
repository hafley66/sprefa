:- module(dl7_dbsp_plan_mainer, [main/0, main/1]).

:- use_module(library(http/json), [json_write_dict/3]).
:- use_module('../2_comptime/2_compiler', [compile_dl7_project_rows/6]).
:- use_module('../3_emit/1a_dbsp_plan_emitter', [emit_dbsp_plan/3]).
:- use_module('0_source_query_mainer', [extract_tsi_rows/4]).

:- initialization(main, main).

main :-
    current_prolog_flag(argv, Arguments),
    main(Arguments).

main(Arguments) :-
    catch(driver_exit_code(Arguments, Code),
          Error,
          ( write_term(user_error, implementation_failure(Error),
                       [quoted(true), fullstop(true), nl(true)]),
            Code = 1
          )),
    halt(Code).

driver_exit_code([ProgramPath, SourcePath | Options], Code) :-
    !,
    dl7_source_query_mainer:default_extract(DefaultExtract),
    dl7_source_query_mainer:parse_options(Options, DefaultExtract, Extract),
    extract_tsi_rows(Extract, SourcePath, TsiRows, ExtractDiagnostics),
    emit_after_extract(ExtractDiagnostics, ProgramPath, TsiRows, Code).
driver_exit_code(_, 2) :-
    format(user_error,
           'usage: dl7-dbsp-plan PROGRAM SOURCE [--extract BIN]~n', []).

emit_after_extract([], ProgramPath, TsiRows, Code) :-
    !,
    once(absolute_file_name(ProgramPath, AbsoluteProgram,
                            [access(read), file_errors(error)])),
    file_directory_name(AbsoluteProgram, Root),
    compile_dl7_project_rows(
        Root, [AbsoluteProgram], TsiRows,
        _, Runtime, CompileDiagnostics),
    emit_after_compile(CompileDiagnostics, Runtime, Code).
emit_after_extract(Diagnostics, _, _, 1) :-
    maplist(write_diagnostic, Diagnostics).

emit_after_compile([], Runtime, Code) :-
    !,
    emit_dbsp_plan(Runtime, Plan, EmitDiagnostics),
    emit_plan(EmitDiagnostics, Plan, Code).
emit_after_compile(Diagnostics, _, 1) :-
    maplist(write_diagnostic, Diagnostics).

emit_plan([], Plan, 0) :-
    !,
    json_write_dict(current_output, Plan, [width(0)]),
    nl.
emit_plan(Diagnostics, _, 1) :-
    maplist(write_diagnostic, Diagnostics).

write_diagnostic(Diagnostic) :-
    write_term(user_error, Diagnostic,
               [quoted(true), ignore_ops(true), numbervars(true),
                fullstop(true), nl(true)]).
