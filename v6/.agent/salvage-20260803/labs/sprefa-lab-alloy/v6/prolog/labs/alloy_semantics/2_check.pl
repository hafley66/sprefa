% 2_check.pl : invariant goals. Failure throws codegen_refused(<reason>),
% mirroring the compiler's unmapped_feature named-refusal style.
%
% Invariants, checked before any text is emitted:
%   1. every ref has exactly one decl            -> codegen_refused(unresolved_ref)
%   2. no two decls share a rendered name / target file
%                                                -> codegen_refused(duplicate_name)
%   3. every import emitted is backed by a cross-file ref (no self-import)
%                                                -> codegen_refused(unused_import)
%
% The compiler refuses by a NAMED reason (unmapped_feature) rather than a bare
% exception; check_all mirrors that: a single codegen_refused/1 term carrying
% the named reason plus the offending addresses.

:- module(alloy_check,
          [ check_all/0,
            check_target/1
          ]).

:- use_module('0_facts').
:- use_module('1_collect').

check_all :-
    forall(target(Target), check_target(Target)).

target(ts).
target(rust).

check_target(Target) :-
    check_unresolved(Target),
    check_duplicate_names(Target),
    check_imports_used(Target).

% -- invariant 1: every ref resolves to exactly one decl -----------------------
check_unresolved(Target) :-
    forall(ref(Target, FromFile, Id),
           ( aggregate_all(count, decl(Target, Id, _, _), N),
             ( N =:= 1
             -> true
             ;  throw(codegen_refused(unresolved_ref(FromFile, Id, N))) ) )).

% -- invariant 2: one rendered name per target file ----------------------------
%
% The duplicate sabotage asserts an existing (File, Name) pair again, so a
% second decl with the same rendered name appears in the same target file.
check_duplicate_names(Target) :-
    findall(File-Name, (decl(Target, Id, _, File), rendered_name(Target, Id, Name)), Pairs0),
    ( duplicate_sabotaged
    -> duplicate_extra(Pairs0, Pairs)
    ;  Pairs = Pairs0
    ),
    ( duplicate_pair(Pairs, File, Name)
    -> throw(codegen_refused(duplicate_name(File, Name)))
    ;  true
    ).

duplicate_extra([File-Name|Rest], [File-Name, File-Name|Rest]).

duplicate_pair(Pairs, File, Name) :-
    select(File-Name, Pairs, Rest),
    member(File-Name, Rest).

duplicate_sabotaged :-
    getenv('ALLOW_LAB_SABOTAGE_DUPLICATE', V), V \== ''.

% -- invariant 3: every import is backed by a cross-file ref --------------------
%
% import_needed is derived from ref/2 where FromFile /= decl file, so this can
% only fail on a self-import (a file importing a symbol it itself declares),
% which is a misuse of the import line. Trivially satisfied on a healthy base.
check_imports_used(Target) :-
    forall(import_needed(Target, FromFile, _ToFile, Id),
           ( decl(Target, Id, _, _),
             ( member_in_file(Target, Id, FromFile)
             -> throw(codegen_refused(unused_import(FromFile, Id)))
             ;  true ) )).

member_in_file(Target, Id, File) :-
    decl(Target, Id, _, File).
