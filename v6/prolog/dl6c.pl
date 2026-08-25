% @comment-ok: the CLI's single documentation site (usage, exit codes, the
% option-library choice), same family as compile.pl:95's plan/9 field contract.
%
%   dl6c <in.dl6> --target rust|ts --out <dir>
%   dl6c --version
%
% Wraps the same compile_dl6/3 call grade.sh:36 and compile/scripts/compile_dl6.sh
% make. Exit codes are `bop check`'s contract (compile/scripts/bop_check.pl):
% 0 compiled, 2 a named unsupported construct, 1 anything else.
%
% Options parse through library(main)'s argv_options/3 (opt_type/3 + opt_help/2
% below), not library(optparse), so the entry point main/0 and the option table
% come from one library.

:- module(dl6c,
          [ main/0,
            main/1,
            dl6c_save/1,
            dl6c_version/1,
            named_reason_functor/1
          ]).

:- use_module(library(main), [argv_options/3, argv_usage/1]).
:- use_module(library(lists), [memberchk/2]).
:- use_module(library(filesex), [directory_file_path/3, make_directory_path/1]).
:- use_module(compile, [compile_dl6/3]).
:- use_module('1_expansion/compile_messages', []).
:- use_module(emit_ts, []).
:- use_module(emit_rust, []).

% Stamped by dl6c_save/1 from DL6C_BUILD_SHA into the saved state; an unsaved
% `swipl -l dl6c.pl` session answers `unknown`.
:- dynamic dl6c_build_sha/1.

main :-
    current_prolog_flag(argv, Argv),
    main(Argv).

% Compute one integer, then halt/1 once outside every catch: SWI represents
% halt/1 as an unwind exception, so halting inside a catch would be caught.
main(Argv) :-
    exit_code(Argv, Code),
    halt(Code).

exit_code(Argv, Code) :-
    argv_options(Argv, Positional, Options),
    (   memberchk(version(true), Options)
    ->  print_version,
        Code = 0
    ;   Positional = [Input],
        memberchk(target(Target), Options),
        memberchk(out(OutDir), Options)
    ->  compile_exit_code(Input, Target, OutDir, Code)
    ;   argv_usage(error),
        Code = 1
    ).

opt_type(target,  target,  oneof([rust, ts])).
opt_type(out,     out,     atom).
opt_type(version, version, boolean).

opt_help(target,  "emit target, rust or ts").
opt_help(out,     "directory the emitted module is written into").
opt_help(version, "print the sha this executable was built from").
opt_help(help(usage), " <in.dl6> --target rust|ts --out <dir>").

opt_meta(target, 'TARGET').
opt_meta(out,    'DIR').

target_emitter(ts,   emit_ts:emit_program,   ts).
target_emitter(rust, emit_rust:emit_program, rs).

% compile_dl6/3 names the emitted program from the INPUT base name, never from
% the output path, so `--out` picks the write location only.
output_file(Input, OutDir, Extension, OutFile) :-
    file_base_name(Input, BaseName),
    file_name_extension(Name, _, BaseName),
    file_name_extension(Name, Extension, OutName),
    make_directory_path(OutDir),
    directory_file_path(OutDir, OutName, OutFile).

compile_exit_code(Input, Target, OutDir, Code) :-
    target_emitter(Target, Emitter, Extension),
    catch(
        (   output_file(Input, OutDir, Extension, OutFile),
            (   compile_dl6(Input, OutFile, [emitter(Emitter)])
            ->  Result = clean
            ;   Result = broken(compile_failed(Input))
            )
        ),
        Error,
        Result = from_error(Error)),
    result_code(Result, Code).

result_code(clean, 0).
result_code(broken(Reason), 1) :-
    print_rendered_error("broken", Reason).
result_code(from_error(Error), Code) :-
    (   compound(Error),
        functor(Error, Functor, _),
        named_reason_functor(Functor)
    ->  print_rendered_error("unsupported", Error),
        Code = 2
    ;   print_rendered_error("broken", Error),
        Code = 1
    ).

print_rendered_error(Prefix, Error) :-
    message_to_string(Error, Text),
    print_message(error, dl6_cli_error(Prefix, Text)).

% Kept in step BY HAND with bop_check.pl's list of the same name;
% compile/test/dl6c.test.pl asserts the two are set-equal.
named_reason_functor(unsupported_construct).
named_reason_functor(not_stratified).
named_reason_functor(column_mismatch).
named_reason_functor(bind_mismatch).
named_reason_functor(bind_and_rule_head).
named_reason_functor(probe_mismatch).
named_reason_functor(query_mismatch).
named_reason_functor(refused_host_decl).
named_reason_functor(template_mismatch).
named_reason_functor(unmapped_feature).

dl6c_version(Version) :-
    (   dl6c_build_sha(Sha)
    ->  Version = Sha
    ;   Version = unknown
    ).

print_version :-
    dl6c_version(Version),
    format("dl6c ~w~n", [Version]).

% autoload(true) resolves every autoloadable predicate the compiler reaches
% INTO the state, which is what lets it run with no v6/prolog on any path.
dl6c_save(Path) :-
    (   getenv('DL6C_BUILD_SHA', Sha)
    ->  true
    ;   Sha = unknown
    ),
    retractall(dl6c_build_sha(_)),
    assertz(dl6c_build_sha(Sha)),
    qsave_program(Path,
                  [ stand_alone(true),
                    goal(dl6c:main),
                    toplevel(halt),
                    autoload(true),
                    op(save)
                  ]).
