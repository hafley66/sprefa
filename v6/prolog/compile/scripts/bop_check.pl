% bop_check.pl : the bop CLI's `check` verb (registry.pl cli_command(check,
% ...)). Validates one `.dl6` file through the SAME text door compile_dl6/2
% uses (parse_dl_file/4 then compile_program/6 with the real emitter), never
% a parallel validation path, and writes its answer as an EXIT CODE per the
% user's CLI contract:
%
%   0  clean    -- zero parse findings, compiles without a named refusal
%   1  broken   -- the file does not parse at all (parse_dl_file FAILS, or
%                  throws something that is not one of the compiler's own
%                  named-reason exceptions below), or any other uncaught
%                  fault during compilation
%   2  findings -- parse_dl_file returned unsupported_surface(...) findings,
%                  OR the compile pipeline threw one of its own NAMED
%                  refusal reasons
%
% "Named refusal reason" is a closed list, grepped once
% (`grep -rhoE "throw\([a-z_]+" v6/prolog/compile/*.pl`) rather than guessed:
% every throw shape the analyzer/lowerer/hosts pipeline uses to name a
% specific refused construct. Anything else escaping to the top (a swipl
% type_error, existence_error, or a genuinely unparseable file) is NOT one of
% these -- it is broken, not a finding, and exits 1.
%
% v6/tsv2/cli/bop.ts's `classifyCompileFailureText` mirrors this same list
% for the `run`/`load` verbs, which hit this same compile door over HTTP and
% only see the thrown term's TEXT (the http body), never the term itself --
% see that function's own header for why the two lists are kept in step by
% hand instead of sharing one file.
%
% HALT DISCIPLINE, the reason for the exit_code/2 + halt/1 split below: SWI
% implements halt/1 as `throw(unwind(halt(Code)))` internally, so a bare
% `catch(Goal, Error, Recovery)` with an unbound Error INSIDE Goal intercepts
% its own halt call the moment one fires (measured: the first draft's
% clean-exit path landed in the error handler as `unwind(halt(0))`, printed
% "broken", and exited 1). The fix is structural, not a pattern exclusion:
% every code path below COMPUTES a plain integer, and halt/1 is called
% exactly ONCE, at the very top, outside every catch/3 in this file.
%
% Run: BOP_CHECK_FILE=/abs/path/to/prog.dl6 swipl -q -l bop_check.pl \
%        -g bop_check_env -g halt
%
% (Argument travels through an environment variable, not string-interpolated
% into a prolog goal atom, so a file path containing a quote character can
% never break the invocation the way `-g "bop_check('$FILE')"` would.)
%
% SABOTAGE RECEIPT (run at authoring time, reverted): flipping
% `result_code(from_error(Error), Code)`'s refusal branch from `Code = 2` to
% `Code = 1` made `ghcacher.dl6` (a real fixture that hits
% `unsupported_construct(recursive_stratum(...))`) exit 1 instead of 2;
% v6/tsv2/tests/bopCheck.test.ts's findings-fixture assertion goes red
% against that mutant and green again once reverted, so the exit-code
% mapping is a receipt, not an assumption.

:- module(bop_check, [bop_check/1, bop_check_env/0]).

:- use_module('../compile', [compile_program/6]).
:- use_module('../parse_dl', [parse_dl_file/4]).
:- use_module(library(lists)).

bop_check_env :-
    getenv('BOP_CHECK_FILE', File),
    bop_check(File).

bop_check(File) :-
    exit_code(File, Code),
    halt(Code).

exit_code(File, Code) :-
    catch(
        ( check_result(File, Result) -> true ; Result = broken(parse_failed(File)) ),
        Error,
        Result = from_error(Error)
    ),
    result_code(Result, Code).

check_result(File, Result) :-
    parse_dl_file(File, Prog, Bindings, Findings),
    ( Findings \== []
    -> Result = findings(Findings)
    ;  compile_pure(File, Prog, Bindings), Result = clean
    ).

% Writes the emitted module to a throwaway temp file and deletes it -- `check`
% validates, it does not keep an output artifact (that is `bop load`'s job,
% over http, or plain compile_dl6.sh for a kept file on disk). compile_program/6
% itself prints `wrote <path>` to current_output on success; `with_output_to`
% swallows that (the temp path is not information a caller of `check` wants),
% leaving stderr as the only channel this script writes on purpose.
compile_pure(File, Prog, Bindings) :-
    file_base_name(File, BaseName),
    file_name_extension(Name, _Extension, BaseName),
    tmp_file_stream(text, TmpFile, TmpStream), close(TmpStream),
    setup_call_cleanup(
        true,
        with_output_to(string(_),
            compile_program(Name, fixture(Name, Prog, [], [], []), Bindings, [], TmpFile, emit_ts:emit_program)),
        catch(delete_file(TmpFile), _, true)).

result_code(clean, 0).
result_code(findings(Findings), 2) :-
    forall(member(Finding, Findings), print_rendered_error("finding", Finding)).
result_code(broken(Reason), 1) :-
    print_rendered_error("broken", Reason).
result_code(from_error(Error), Code) :-
    ( compound(Error), functor(Error, Functor, _), named_reason_functor(Functor)
    -> print_rendered_error("refusal", Error), Code = 2
    ;  print_rendered_error("broken", Error), Code = 1
    ).

print_rendered_error(Prefix, Error) :-
    message_to_string(Error, Text),
    format(user_error, "~w: ~w~n", [Prefix, Text]).

% Every throw shape the compile pipeline (analyze.pl / lower.pl / 1_host_expand.pl
% / compile.pl) uses to name one specific refused construct. Kept as a flat
% fact list, not derived from registry.pl's refused rows, because several of
% these fire on shapes registry.pl never sees at all (arity mismatches,
% cross-plane checks) -- this is the exception VOCABULARY, not the construct
% inventory.
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
