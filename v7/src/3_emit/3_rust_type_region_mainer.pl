:- module(dl7_rust_type_region_mainer,
          [ refresh_rust_type_region/7,
            main/0
          ]).

:- use_module(library(http/json), [atom_json_dict/3]).
:- use_module(library(process), [process_create/3, process_wait/2]).
:- use_module('../2_comptime/0c_extract_loader', [load_tsi_text/3]).
:- use_module('2_rust_type_emitter', [render_rust_type_rows/4]).

%% refresh_rust_type_region(+Extract, +RustPath, +TargetPath, +Region,
%%                          +Mode, -Status, -Diagnostics) is det.
%
% Mode is `check` or `apply(State)`, where State is `default` or a path.
refresh_rust_type_region(Extract, RustPath, TargetPath, Region, Mode,
                         Status, Diagnostics) :-
    extract_type_stream(Extract, RustPath, StreamText,
                        ExtractDiagnostics),
    continue_after_extract(
        ExtractDiagnostics, Extract, RustPath, TargetPath, Region, Mode,
        StreamText, Status, Diagnostics).

continue_after_extract([], Extract, RustPath, TargetPath, Region, Mode,
                       StreamText, Status, Diagnostics) :-
    !,
    load_tsi_text(StreamText, Rows, LoadDiagnostics),
    continue_after_load(
        LoadDiagnostics, Extract, RustPath, TargetPath, Region, Mode,
        Rows, Status, Diagnostics).
continue_after_extract(Diagnostics, _, _, _, _, _, _, failed, Diagnostics).

continue_after_load([], Extract, RustPath, TargetPath, Region, Mode,
                    Rows, Status, Diagnostics) :-
    !,
    render_rust_type_rows(RustPath, Rows, Generated, RenderDiagnostics),
    continue_after_render(
        RenderDiagnostics, Extract, TargetPath, Region, Mode, Generated,
        Status, Diagnostics).
continue_after_load(Diagnostics, _, _, _, _, _, failed, Diagnostics).

continue_after_render([], Extract, TargetPath, Region, Mode, Generated,
                      Status, Diagnostics) :-
    !,
    region_command(Mode, TargetPath, Region, Arguments),
    run_process(Extract, Arguments, Generated, Exit, Output, Error),
    decode_region_result(Exit, Output, Error, Status, Diagnostics).
continue_after_render(Diagnostics, _, _, _, _, _, failed, Diagnostics).

extract_type_stream(Extract, RustPath, StreamText, Diagnostics) :-
    run_process(Extract,
                ['--witness', '--family', type, RustPath],
                "", Exit, StreamText, Error),
    (   Exit =:= 0
    ->  Diagnostics = []
    ;   Diagnostics = [diagnostic(extract, file(RustPath),
                                  process_exit(Exit, Error))]
    ).

region_command(check, TargetPath, Region,
               [region, TargetPath, Region]).
region_command(apply(default), TargetPath, Region,
               [region, TargetPath, Region, '--apply']).
region_command(apply(State), TargetPath, Region,
               [region, TargetPath, Region, '--state', State, '--apply']) :-
    State \== default.

run_process(Executable, Arguments, Input, Exit, Output, Error) :-
    process_create(Executable, Arguments,
                   [ stdin(pipe(In)),
                     stdout(pipe(Out)),
                     stderr(pipe(Err)),
                     process(Pid)
                   ]),
    format(In, '~s', [Input]),
    close(In),
    read_string(Out, _, Output),
    close(Out),
    read_string(Err, _, Error),
    close(Err),
    process_wait(Pid, exit(Exit)).

decode_region_result(Exit, Output, Error, Status, Diagnostics) :-
    normalize_space(string(Json), Output),
    (   catch(atom_json_dict(Json, Dict, [value_string_as(string)]), _, fail),
        get_dict(status, Dict, StatusText),
        string(StatusText),
        atom_string(Status, StatusText),
        valid_region_exit(Status, Exit)
    ->  Diagnostics = []
    ;   Status = failed,
        Diagnostics = [diagnostic(output, none,
                                  region_process_exit(Exit, Error, Output))]
    ).

valid_region_exit(drift, 1).
valid_region_exit(current, 0).
valid_region_exit(applied, 0).

main :-
    current_prolog_flag(argv, Arguments),
    (   parse_arguments(Arguments, Extract, RustPath, TargetPath,
                        Region, Mode)
    ->  refresh_rust_type_region(
            Extract, RustPath, TargetPath, Region, Mode,
            Status, Diagnostics),
        print_result(Status, Diagnostics),
        status_exit(Status, Exit),
        halt(Exit)
    ;   format(user_error,
               'usage: dl7-rust-types RUST TARGET REGION [--apply] [--extract BIN] [--state DIR]~n',
               []),
        halt(2)
    ).

parse_arguments([RustPath, TargetPath, Region | Options],
                Extract, RustPath, TargetPath, Region, Mode) :-
    default_extract(Extract0),
    parse_options(Options, Extract0, Extract,
                  false, Apply, default, State),
    refresh_mode(Apply, State, Mode).

parse_options([], Extract, Extract, Apply, Apply, State, State).
parse_options(['--apply' | Options], Extract0, Extract, _, Apply,
              State0, State) :-
    parse_options(Options, Extract0, Extract, true, Apply, State0, State).
parse_options(['--extract', Binary | Options], _, Extract, Apply0, Apply,
              State0, State) :-
    parse_options(Options, Binary, Extract, Apply0, Apply, State0, State).
parse_options(['--state', Requested | Options], Extract0, Extract,
              Apply0, Apply, _, State) :-
    parse_options(Options, Extract0, Extract,
                  Apply0, Apply, Requested, State).

refresh_mode(false, _, check).
refresh_mode(true, State, apply(State)).

default_extract(Extract) :-
    (   getenv('SPREFA_EXTRACT_BIN', Extract)
    ->  true
    ;   Extract = 'v6/sprefa-extract/target/debug/extract'
    ).

print_result(Status, []) :-
    format('~w~n', [Status]).
print_result(_, Diagnostics) :-
    forall(member(Diagnostic, Diagnostics),
           format(user_error, '~q~n', [Diagnostic])).

status_exit(drift, 1) :- !.
status_exit(failed, 2) :- !.
status_exit(_, 0).

:- initialization(main, main).
