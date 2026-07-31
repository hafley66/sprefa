% Conformance runner for the reference interpreter and fixture files.
%
% Run: swipl -q -l v6/prolog/conformance/go.pl -g go -g halt

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

:- use_module('../src/grader.pl').
:- use_module(engine).

% Every fixtures/*.pl file is loaded in sorted order.
load_fixture_files :-
    prolog_load_context(directory, Here),
    atomic_list_concat([Here, '/fixtures'], FixturesDir),
    directory_files(FixturesDir, Entries),
    msort(Entries, Ordered),
    forall(( member(Entry, Ordered), sub_atom(Entry, _, 3, 0, '.pl') ),
           ( atomic_list_concat([FixturesDir, '/', Entry], Path),
             ensure_loaded(Path) )).

:- load_fixture_files.

check(Name, engine:fixture_expectations_hold(Name, Expectations)) :-
    fixture(Name, _, _, _, Expectations).

go :- run(check).
