:- module(dl7_rust_region_mainer, [main/0, refresh_dl7_rust_region/7]).

:- use_module(library(http/json), [atom_json_dict/3]).
:- use_module(library(process), [process_create/3, process_wait/2]).
:- use_module('../3_emit/2a_dl7_rust_emitter', [render_dl7_rust_file/3]).

:- initialization(main, main).

refresh_dl7_rust_region(Extract, Program, Target, Region, Mode,
                        Status, Diagnostics) :-
    render_dl7_rust_file(Program, Generated, EmitDiagnostics),
    continue_after_emit(
        EmitDiagnostics, Extract, Target, Region, Mode, Generated,
        Status, Diagnostics).

continue_after_emit([], Extract, Target, Region, Mode, Generated,
                    Status, Diagnostics) :-
    !,
    region_arguments(Mode, Target, Region, Arguments),
    run_region(Extract, Arguments, Generated, Exit, Output, Error),
    decode_region_result(Exit, Output, Error, Status, Diagnostics).
continue_after_emit(Diagnostics, _, _, _, _, _, failed, Diagnostics).

region_arguments(check, Target, Region, [region, Target, Region]).
region_arguments(apply(default), Target, Region,
                 [region, Target, Region, '--apply']).
region_arguments(apply(State), Target, Region,
                 [region, Target, Region, '--state', State, '--apply']) :-
    State \== default.

run_region(Executable, Arguments, Input, Exit, Output, Error) :-
    process_create(Executable, Arguments,
                   [stdin(pipe(In)), stdout(pipe(Out)), stderr(pipe(Err)),
                    process(Pid)]),
    format(In, '~s', [Input]),
    close(In),
    read_string(Out, _, Output),
    close(Out),
    read_string(Err, _, Error),
    close(Err),
    process_wait(Pid, exit(Exit)).

decode_region_result(Exit, Output, Error, Status, Diagnostics) :-
    normalize_space(string(Json), Output),
    ( catch(atom_json_dict(Json, Dict, [value_string_as(string)]), _, fail),
      get_dict(status, Dict, StatusText),
      atom_string(Status, StatusText),
      valid_exit(Status, Exit)
    -> Diagnostics = []
    ;  Status = failed,
       Diagnostics = [diagnostic(output, none,
                                 region_process_exit(Exit, Error, Output))]
    ).

valid_exit(drift, 1).
valid_exit(current, 0).
valid_exit(applied, 0).

main :-
    current_prolog_flag(argv, Arguments),
    ( parse_arguments(Arguments, Program, Target, Region,
                      Extract, Mode)
    -> refresh_dl7_rust_region(
           Extract, Program, Target, Region, Mode,
           Status, Diagnostics),
       print_result(Status, Diagnostics),
       status_exit(Status, Exit),
       halt(Exit)
    ;  format(user_error,
              'usage: dl7-rust-types PROGRAM TARGET REGION [--apply] [--extract BIN] [--state PATH]~n',
              []),
       halt(2)
    ).

parse_arguments([Program, Target, Region | Options],
                Program, Target, Region, Extract, Mode) :-
    default_extract(DefaultExtract),
    parse_options(Options, DefaultExtract, Extract,
                  false, Apply, default, State),
    ( Apply == true -> Mode = apply(State) ; Mode = check ).

parse_options([], Extract, Extract, Apply, Apply, State, State).
parse_options(['--apply' | Options], Extract0, Extract, _, Apply,
              State0, State) :-
    parse_options(Options, Extract0, Extract, true, Apply, State0, State).
parse_options(['--extract', Binary | Options], _, Extract, Apply0, Apply,
              State0, State) :-
    parse_options(Options, Binary, Extract, Apply0, Apply, State0, State).
parse_options(['--state', Path | Options], Extract0, Extract,
              Apply0, Apply, _, State) :-
    parse_options(Options, Extract0, Extract,
                  Apply0, Apply, Path, State).

default_extract(Extract) :-
    ( getenv('SPREFA_EXTRACT_BIN', Extract)
    -> true
    ;  Extract = 'v6/sprefa-extract/target/debug/extract'
    ).

print_result(Status, []) :-
    format('~w~n', [Status]).
print_result(_, Diagnostics) :-
    forall(member(Diagnostic, Diagnostics),
           write_term(user_error, Diagnostic,
                      [quoted(true), fullstop(true), nl(true)])).

status_exit(drift, 1) :- !.
status_exit(failed, 2) :- !.
status_exit(_, 0).
