// Meta-spec rows 1-5: setof/bagof template-caret-goal, aggregate_all/4 goal at
// index 2, catch_with_backtrace/3, setup_call_catcher_cleanup/4, partition
// closure and include/3. Closure targets: p/2 (partition, include), p/3 (foldl).
:- module(meta_specs, []).

t1(L) :- setof(p(X), q(X), L).
t2(L) :- bagof(p(X), r(X), L).
t3(_) :- aggregate_all(count, X, q(X), _).
t4(G) :- catch_with_backtrace(q(G), _, r(G)).
t5 :- setup_call_catcher_cleanup(q(a), r(G), _, r(G)).
t6(L) :- partition(p, L, _, _, _).
t7(S, A) :- include(p, S, A).
t8(A) :- foldl(p, [1,2], 0, A).

q(_). r(_).
p(X, Y) :- X > Y.
p(_).
