% prolog_lint.pl -- read-only organization gate over v6/prolog/**/*.pl.
%
% Implements the 8-step recipe from plans/2026-07-29-prolog-org-review.md
% section 6. Three SWI processes are needed because the tree cannot be loaded
% all at once: two source clusters declare their own modules, and the loaded
% checks in library(check) only see code that is actually loaded.
%
%   lint_sources          steps 1, 2, 8  -- xref every file, duplicate module
%                                            names, unused-export advisory
%   lint_loaded(compile)  steps 3 to 6   -- load the plunit cluster, then
%                                            undefined / cross-module /
%                                            redefined / void / trivial-fail /
%                                            format checks
%   lint_loaded(example)  step 7         -- same checks over examples/ghcacher
%
% Output protocol, consumed by tools/prolog-lint.sh:
%   FINDING<TAB><class><TAB><detail>     gate-failing, ratcheted by a baseline
%   ADVISORY<TAB><class><TAB><detail>    printed, never fails the gate
%
% Finding details are normalized to be position independent: no clause
% references, no line numbers, repository-relative paths only. A PLUnit test
% body reports as its unit name with the `@line N` suffix stripped, so that
% inserting a test above another one does not churn the baseline.

:- module(prolog_lint,
          [ lint_sources/0,
            lint_loaded/1
          ]).

:- use_module(library(prolog_xref)).
:- use_module(library(check)).
:- use_module(library(lists)).
:- use_module(library(apply)).

:- dynamic captured_message/1.
:- dynamic source_module_of/2.
:- dynamic export_of/2.
:- dynamic call_in/2.

% ── locating the tree ────────────────────────────────────────────────────────

% This file is <prolog_root>/tools/prolog_lint.pl.
prolog_root(Root) :-
    module_property(prolog_lint, file(Self)),
    file_directory_name(Self, ToolsDir),
    file_directory_name(ToolsDir, Root).

relative_source(Absolute, Relative) :-
    prolog_root(Root),
    atom_concat(Root, '/', Prefix),
    (   atom_concat(Prefix, Relative, Absolute)
    ->  true
    ;   Relative = Absolute
    ).

source_files(Files) :-
    prolog_root(Root),
    findall(File,
            ( directory_member(Root, File,
                               [ recursive(true),
                                 extensions([pl])
                               ]) ),
            Unsorted),
    sort(Unsorted, Files).

% ── message capture ──────────────────────────────────────────────────────────
%
% library(check) reports through print_message/2. The hook records the term and
% fails so the human-readable line still reaches stderr for interactive runs.

user:message_hook(Term, Kind, _Lines) :-
    memberchk(Kind, [warning, error]),
    assertz(prolog_lint:captured_message(Term)),
    fail.

reset_capture :-
    retractall(captured_message(_)).

% ── output ───────────────────────────────────────────────────────────────────

finding(Class, Detail) :-
    format("FINDING\t~w\t~w~n", [Class, Detail]).

advisory(Class, Detail) :-
    format("ADVISORY\t~w\t~w~n", [Class, Detail]).

% ── steps 1, 2, 8: source-level cross reference ──────────────────────────────

lint_sources :-
    source_files(Files),
    retractall(source_module_of(_, _)),
    retractall(export_of(_, _)),
    retractall(call_in(_, _)),
    maplist(xref_one, Files),
    report_duplicate_modules,
    report_unused_exports,
    length(Files, Count),
    advisory(files_cross_referenced, Count).

xref_one(File) :-
    reset_capture,
    catch(xref_source(File, [silent(true)]), Error, true),
    relative_source(File, Relative),
    (   nonvar(Error)
    ->  message_to_text(Error, Text),
        finding(parse_error, Relative-Text)
    ;   true
    ),
    forall(captured_message(Message),
           report_xref_message(Relative, Message)),
    (   xref_module(File, Module)
    ->  assertz(source_module_of(Relative, Module))
    ;   true
    ),
    forall(xref_exported(File, Callable),
           ( indicator(Callable, Indicator),
             assertz(export_of(Relative, Indicator)) )),
    forall(xref_called(File, Called, _By),
           ( indicator(Called, Indicator),
             ( call_in(Relative, Indicator) -> true
             ; assertz(call_in(Relative, Indicator)) ) )).

% A syntax error inside a cross referenced file surfaces as an error message
% rather than an exception out of xref_source/2.
report_xref_message(Relative, error(syntax_error(Which), _)) :-
    !,
    finding(parse_error, Relative-Which).
report_xref_message(_, _).

% Parse failures are reported as their raw term. Rendering them through the
% message system would need a private hook into library($messages), which this
% gate itself would then flag as a cross-module call.
message_to_text(Term, Text) :-
    term_to_atom(Term, Text).

indicator(Module:Callable, Indicator) :-
    !,
    indicator(Callable, Bare),
    Indicator = Module:Bare.
indicator(Callable, Name/Arity) :-
    functor(Callable, Name, Arity).

report_duplicate_modules :-
    findall(Module, source_module_of(_, Module), All),
    sort(All, Distinct),
    forall(( member(Module, Distinct),
             findall(File, source_module_of(File, Module), Files),
             Files = [_, _|_] ),
           ( atomic_list_concat(Files, ' ', Joined),
             finding(duplicate_module, Module-Joined) )).

% Step 8 is advisory only: entry predicates reached through `-g`, ensure_loaded,
% or meta-calls are indistinguishable from dead exports at the source level.
report_unused_exports :-
    forall(( export_of(File, Indicator),
             \+ ( call_in(Other, Called),
                  Other \== File,
                  matches_export(Called, Indicator) ) ),
           advisory(unused_export_candidate, File-Indicator)).

matches_export(Indicator, Indicator) :- !.
matches_export(_Module:Indicator, Indicator).

% ── steps 3 to 7: loaded-code checks ─────────────────────────────────────────

cluster_entry(compile, 'compile/test/plunit_tests.pl').
cluster_entry(example, 'examples/ghcacher.pl').

lint_loaded(Cluster) :-
    cluster_entry(Cluster, RelativeEntry),
    prolog_root(Root),
    atomic_list_concat([Root, '/', RelativeEntry], Entry),
    load_quietly(Entry),
    reset_capture,
    list_undefined([module_class([user, test])]),
    list_cross_module_calls([module_class([user, test])]),
    list_redefined,
    list_void_declarations,
    list_trivial_fails([module_class([user, test])]),
    list_format_errors([module_class([user, test])]),
    forall(captured_message(Message), report_check(Cluster, Message)).

% Both entry files are module-free at the top level, so the load has to be
% qualified into `user`. An unqualified ensure_loaded/1 from inside this module
% would pull their clauses into `prolog_lint` and misattribute every finding.
% The entry files also print banners of their own, so loading is fenced from
% the capture buffer.
load_quietly(Entry) :-
    reset_capture,
    user:ensure_loaded(Entry),
    reset_capture.

report_check(Cluster, check(undefined_procedures, Grouped)) :-
    !,
    forall(member(Group, Grouped), report_undefined(Cluster, Group)).
report_check(Cluster, check(cross_module_call(Target, Caller, _Where))) :-
    !,
    indicator(Target, TargetIndicator),
    caller_label(Caller, CallerLabel),
    finding(private_cross_module_call,
            Cluster-TargetIndicator-CallerLabel).
report_check(Cluster, check(redefined(Type, Module, Indicator))) :-
    !,
    finding(redefined, Cluster-Type-Module-Indicator).
% The reported term is a callable with fresh variables, whose numbering moves
% between runs; only its indicator is stable enough for a baseline.
report_check(Cluster, check(void_declaration(Callable, Declaration))) :-
    !,
    indicator(Callable, Indicator),
    finding(void_declaration, Cluster-Indicator-Declaration).
report_check(Cluster, check(trivial_failure(Goal, _From))) :-
    !,
    indicator(Goal, Indicator),
    finding(trivial_fail, Cluster-Indicator).
report_check(Cluster, check(format_error(Message), Goal, _Context)) :-
    !,
    indicator(Goal, Indicator),
    finding(format_error, Cluster-Indicator-Message).
report_check(_, check(cross_module_calls)).
report_check(_, check(trivial_failures)).
report_check(_, _).

report_undefined(Cluster, Indicator-_References) :-
    !,
    finding(undefined_predicate, Cluster-Indicator).
report_undefined(Cluster, Other) :-
    finding(undefined_predicate, Cluster-Other).

% `unit body`('name@line 355', vars) carries a line number that moves whenever
% a test is inserted above it. The baseline stays stable on the unit name.
caller_label(Module:Head, Module-Label) :-
    !,
    head_label(Head, Label).
caller_label(Other, Other).

head_label('unit body'(Name, _Vars), Stripped) :-
    !,
    strip_line_suffix(Name, Stripped).
head_label(Head, Indicator) :-
    indicator(Head, Indicator).

strip_line_suffix(Name, Stripped) :-
    atom(Name),
    sub_atom(Name, Before, _, _, '@line '),
    !,
    sub_atom(Name, 0, Before, _, Stripped).
strip_line_suffix(Name, Name).
