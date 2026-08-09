% compile_all.pl : compile ONE generated cell through the SAME text door a
% user's file goes through (compile:compile_dl6/2) and print a one-line JSON
% verdict. drive.mjs invokes this once per cell.
%
% ONE PROCESS PER CELL, deliberately, after measuring: loading the compiler
% front costs 0.09s, so a batch loop saves about a minute over the whole matrix
% and would lose crash isolation -- and the ORACLE door next door calls halt/1
% on a malformed arrival, which no in-process catch can survive. Paying the
% same shape on both doors keeps one driver.
%
% The door itself is unchanged: this file calls the exported predicate.
%
% Run: swipl -q -l compile_all.pl -g "one(cell_arrival__int__int)" -g halt

:- use_module('../../compile', [compile_dl6/2]).
:- use_module(library(http/json)).

one(Id) :-
    atomic_list_concat(['out/', Id, '.dl6'], Source),
    atomic_list_concat(['out/', Id, '.ts'], Target),
    (   catch(with_output_to(string(_Noise), compile_dl6(Source, Target)),
              Error, true)
    ->  ( var(Error)
        -> Status = ok, Detail = ''
        ;  unsupported_detail(Error, Status, Detail) )
    ;   Status = error, Detail = "compile_dl6 failed without throwing"
    ),
    print_result(Status, Detail).

% WHAT COUNTS AS A NAMED REFUSAL, and why the rule is this one. The obvious
% rule -- `unsupported_construct/1` or `unsupported_surface/1`, the
% 0_unsupported_messages.pl vocabulary -- was MEASURED WRONG on this corpus: the
% reference engine throws `type_arrival_shape_mismatch/4` and
% `json_capture_type_unknown/1` BARE, and the parser throws
% `dl_parse_error/2`, so a vocabulary test called 158 named answers "unnamed
% crash". The rule that survives contact: an ISO `error/2` term is a crash,
% and any other thrown compound is a named answer whose functor is its name.
% That the two doors' unsupported constructs do not share one wrapper is a finding, not a
% harness detail (those bare terms have no prolog:message//1 clause, so a cold
% author sees swipl's "Unknown message").
unsupported_detail(error(Formal, Context), error, Detail) :- !,
    term_string(error(Formal, Context), Detail).
unsupported_detail(Term, refused, Detail) :-
    compound(Term), !,
    term_string(Term, Detail).
unsupported_detail(Term, error, Detail) :-
    term_string(Term, Detail).

% The library writes the escaping; a hand-rolled escaper here would be the
% third copy of one in this repo and the first one nothing tests.
print_result(Status, Detail) :-
    with_output_to(string(Line),
                   json_write_dict(current_output,
                                   _{status: Status, detail: Detail},
                                   [width(0)])),
    format('~w~n', [Line]).
