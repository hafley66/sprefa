% causality.pl : reject instantaneous dependency cycles. idep(E, X) holds when
% E reads stream X at the SAME tick; pre cuts the edge (that is what a
% register is for). reach is transitive closure; :- table is what keeps
% prolog from looping on the cyclic case (the datalog move, from inside).
% Run: swipl -q -l causality.pl -g go -g halt

:- use_module(library(lists)).

idep(var(X), X).
idep(plus(A, B), X)  :- ( idep(A, X) ; idep(B, X) ).
idep(arrow(A, B), X) :- ( idep(A, X) ; idep(B, X) ).
idep(fby(A, _), X)   :- idep(A, X).            % second arg is under pre
idep(when(E, S), X)  :- ( idep(E, X) ; X = S ).
idep(current(E), X)  :- idep(E, X).
                                               % const, pre: no clause, no edge
:- table reach/3.

edge(Eqs, From, Dep) :- member(eq(From, Expr), Eqs), idep(Expr, Dep).
reach(Eqs, A, B)     :- edge(Eqs, A, B).
reach(Eqs, A, B)     :- edge(Eqs, A, Mid), reach(Eqs, Mid, B).

causal(Eqs) :- \+ ( member(eq(X, _), Eqs), reach(Eqs, X, X) ).

node(counter, [eq(n, fby(const(0), plus(var(n), const(1))))]).  % legal: fby hides pre
node(broken,  [eq(x, plus(var(x), const(1)))]).                 % x = x + 1

check(counter_ok, ( node(counter, Eqs), causal(Eqs) )).
check(broken_rejected, ( node(broken, Eqs), \+ causal(Eqs) )).

go :- forall(check(N, G),
             ( catch(G, E, (print_message(error, E), fail))
             -> format("PASS  ~w~n", [N]) ; format("fail  ~w~n", [N]) )).
