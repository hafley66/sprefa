:- module(dl7_dbsp_rust_region_mainer,
          [ main/0,
            refresh_dbsp_rust_region/7
          ]).

:- use_module('../2_comptime/2_compiler', [compile_dl7/4]).
:- use_module('../3_emit/1b_dbsp_rust_emitter', [render_dbsp_rust/3]).
:- use_module('2_dl7_rust_region_mainer',
              [ refresh_generated_region/7,
                parse_arguments/6,
                print_result/2,
                status_exit/2
              ]).

:- initialization(main, main).

refresh_dbsp_rust_region(Extract, Program, Target, Region, Mode,
                         Status, Diagnostics) :-
    compile_dl7(Program, _, Runtime, CompileDiagnostics),
    continue_after_compile(
        CompileDiagnostics, Extract, Target, Region, Mode, Runtime,
        Status, Diagnostics).

continue_after_compile([], Extract, Target, Region, Mode, Runtime,
                       Status, Diagnostics) :-
    !,
    render_dbsp_rust(Runtime, Generated, EmitDiagnostics),
    continue_after_emit(
        EmitDiagnostics, Extract, Target, Region, Mode, Generated,
        Status, Diagnostics).
continue_after_compile(Diagnostics, _, _, _, _, _, failed, Diagnostics).

continue_after_emit([], Extract, Target, Region, Mode, Generated,
                    Status, Diagnostics) :-
    !,
    refresh_generated_region(
        Extract, Target, Region, Mode, Generated, Status, Diagnostics).
continue_after_emit(Diagnostics, _, _, _, _, _, failed, Diagnostics).

main :-
    current_prolog_flag(argv, Arguments),
    ( parse_arguments(Arguments, Program, Target, Region, Extract, Mode)
    -> refresh_dbsp_rust_region(
           Extract, Program, Target, Region, Mode, Status, Diagnostics),
       print_result(Status, Diagnostics),
       status_exit(Status, Exit),
       halt(Exit)
    ;  format(user_error,
              'usage: dl7-dbsp-rust PROGRAM TARGET REGION [--apply] [--extract BIN] [--state PATH]~n',
              []),
       halt(2)
    ).
