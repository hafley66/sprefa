% rel_island.pl : `<-` rules as a zeroth-order island inside a prolog file.
%
% Run:     swipl -q -l books/v6/rel_island.pl
% Score:   ?- go.
%
% The mechanism is term_expansion/2, the same reader hook that makes DCG's
% `-->` work: at LOAD TIME every `Head <- Body` clause is rewritten into
%   1. dl_rule/1 data     : the rule as a term, for the sqlite lowering tier
%   2. a tabled twin      : `Head :- Body` under :- table, runnable right here
% and everything else in the file stays ordinary prolog. One file, two
% worlds: `<-` means datalog (ground, terminating, lowerable), everything
% else flies as full prolog around the island.
%
% The demo graph has a CYCLE (3 -> 1). The tabled twin still terminates;
% untabled prolog would loop. That is the zeroth-order guarantee being
% enforced by the twin, matching what the sqlite fixpoint would do.

:- use_module(library(lists)).

:- op(1150, xfx, <-).

:- dynamic dl_rule/1, island_tabled/1.
:- discontiguous dl_rule/1.

% the reader switch: runs once per <- clause, during consult
term_expansion((Head <- Body), Clauses) :-
    functor(Head, Rel, Arity),
    (   island_tabled(Rel/Arity)
    ->  Clauses = [ dl_rule((Head <- Body)), (Head :- Body) ]
    ;   assertz(island_tabled(Rel/Arity)),
        Clauses = [ (:- discontiguous Rel/Arity),
                    (:- table Rel/Arity),
                    dl_rule((Head <- Body)),
                    (Head :- Body) ]
    ).

% ── ordinary prolog world: facts, helpers, IO, whatever ─────────────────────

edge(1, 2).
edge(2, 3).
edge(3, 1).                       % the cycle
edge(7, 8).
root(1).

% ── the datalog island ──────────────────────────────────────────────────────

reach(Node)  <- root(Node).
reach(Child) <- edge(Parent, Child), reach(Parent).

% ── prolog flying around the island: findall, sort, arithmetic, cuts ────────

report(Sorted) :-
    findall(Node, reach(Node), Nodes),
    sort(Nodes, Sorted).

biggest(Max) :-
    report([First | Rest]),
    foldl([X, Acc, Out]>>(Out is max(X, Acc)), Rest, First, Max).

% ── grader ──────────────────────────────────────────────────────────────────

check(island_is_data,  ( findall(Rule, dl_rule(Rule), Rules),
                         length(Rules, 2) )).
check(island_runs,     ( report([1, 2, 3]) )).      % cycle did not loop
check(prolog_flies,    ( biggest(3) )).

go :-
    forall(check(Name, Goal),
           ( catch(Goal, Error, (print_message(error, Error), fail))
           -> format("PASS  ~w~n", [Name])
           ;  format("fail  ~w~n", [Name]) )).
