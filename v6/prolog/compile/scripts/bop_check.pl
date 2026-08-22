% bop_check.pl : the bop CLI's `check` verb (registry.pl cli_command(check,
% ...)). Validates one `.dl6` file through the SAME text door compile_dl6/2
% uses (expand_uses/8 then compile_program/6 with the real emitter), never
% a parallel validation path, and writes its answer as an EXIT CODE per the
% user's CLI contract:
%
%   0  clean    -- zero parse findings, compiles without a named unsupported construct
%   1  broken   -- the file does not parse at all (expand_uses FAILS, or
%                  throws something that is not one of the compiler's own
%                  named-reason exceptions below), or any other uncaught
%                  fault during compilation
%   2  findings -- expand_uses returned unsupported_surface(...) findings,
%                  OR the compile pipeline threw one of its own NAMED
%                  unsupported construct reasons
%
% Named unsupported construct reasons are a closed list. Other uncaught errors are broken.
%
% v6/tsv2/cli/bop.ts's `classifyCompileFailureText` mirrors this same list
% for the `run`/`load` verbs, which hit this same compile door over HTTP and
% only see the thrown term's TEXT (the http body), never the term itself --
% see that function's own header for why the two lists are kept in step by
% hand instead of sharing one file.
%
% Compute a plain integer inside the catches and call halt/1 once outside them;
% SWI represents halt/1 as an unwind exception.
%
% Run: BOP_CHECK_FILE=/abs/path/to/prog.dl6 swipl -q -l bop_check.pl \
%        -g bop_check_env -g halt
%
% (Argument travels through an environment variable, not string-interpolated
% into a prolog goal atom, so a file path containing a quote character can
% never break the invocation the way `-g "bop_check('$FILE')"` would.)
%
% SABOTAGE RECEIPT (run at authoring time, reverted): flipping
% `result_code(from_error(Error), Code)`'s unsupported construct branch from `Code = 2` to
% `Code = 1` made `ghcacher.dl6` (a real fixture that hits
% `unsupported_construct(recursive_stratum(...))`) exit 1 instead of 2;
% v6/tsv2/tests/bopCheck.test.ts's findings-fixture assertion goes red
% against that mutant and green again once reverted, so the exit-code
% mapping is a receipt, not an assumption.
%
% SECOND SABOTAGE RECEIPT (cold-author defect D3, run 2026-07-31, reverted):
% deleting compile_pure/3's catch/3 wrapper below put the CLI back on the
% unlocated unsupported construct it shipped with:
%   unsupported construct: rule-index unavailable: unsupported_construct: compiler refused
%            rule 'log_on_level_headed_rel' for rel 'beat/1' (...)
% bopCheck.test.ts's located-unsupported construct test goes red on exactly the FILE:LINE
% match against that mutant (it asserts the path and the line together, so a
% unsupported construct naming neither, or naming a wrong line, both fail) and green again
% once the wrapper is back.

:- module(bop_check, [bop_check/1, bop_check_env/0]).

:- use_module('../../compile', [compile_program/6, throw_text_door_error/2, dl6_seeded_form/3]).
:- use_module('../../use_resolve', [expand_uses/8]).
:- use_module('../../compile_messages', []).
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
    expand_uses(File, [], [], _Loaded, Prog, _ModuleTable, Bindings, Findings),
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
%
% The catch/3 around compile_program/6 is what gives a unsupported construct its FILE:LINE
% (cold-author defect D3). compile_dl6/2 wraps its own compile in exactly this
% call; this script cannot reuse compile_dl6/2 wholesale because it owns the
% temp output file and needs parse findings as a SEPARATE result from a thrown
% unsupported construct, so it reuses the wrapper instead. Rethrowing keeps the term shape
% every branch below reads: `unsupported_construct(at(File, Line, Reason))` has
% the same functor as the unlocated form, so result_code/2's named-reason test
% and its exit code are untouched, and 0_unsupported_messages.pl's at/3 arm renders
% the location that was already sitting in its message clause unused.
compile_pure(File, Prog, Bindings) :-
    file_base_name(File, BaseName),
    file_name_extension(Name, _Extension, BaseName),
    tmp_file_stream(text, TmpFile, TmpStream), close(TmpStream),
    setup_call_cleanup(
        true,
        catch(
            with_output_to(string(_),
                ( dl6_seeded_form(Prog, Initial, ProgOut),
                  compile_program(Name, fixture(Name, ProgOut, Initial, [], []), Bindings, Initial, TmpFile, emit_ts:emit_program))),
            Error,
            throw_text_door_error(File, Error)),
        catch(delete_file(TmpFile), _, true)).

result_code(clean, 0).
result_code(findings(Findings), 2) :-
    forall(member(Finding, Findings), print_rendered_error("finding", Finding)).
result_code(broken(Reason), 1) :-
    print_rendered_error("broken", Reason).
result_code(from_error(Error), Code) :-
    ( compound(Error), functor(Error, Functor, _), named_reason_functor(Functor)
    -> print_rendered_error("unsupported", Error), Code = 2
    ;  print_rendered_error("broken", Error), Code = 1
    ).

print_rendered_error(Prefix, Error) :-
    message_to_string(Error, Text),
    print_message(error, dl6_cli_error(Prefix, Text)).

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
