% clock_calculus.pl : Lustre clock inference. Same solver as unify_hm.pl,
% different term grammar: clock ::= base | on(Clock, BoolStream).
% Run: swipl -q -l clock_calculus.pl -g go -g halt

:- use_module(library(lists)).

clock(_Env, const(_), _AnyClock).
clock(Env, var(X), C)        :- member(X:C, Env).
clock(Env, plus(A, B), C)    :- clock(Env, A, C), clock(Env, B, C).  % ONE clock
clock(Env, when(E, S), on(C, S)) :- clock(Env, E, C).                % thin time
clock(Env, current(E), C)    :- clock(Env, E, on(C, _)).             % stretch back
clock(Env, merge(S, T, F), C) :-                                     % clock match
    clock(Env, T, on(C, S)),
    clock(Env, F, on(C, not(S))).

check(clash, ( \+ clock([dist:base, second:base],
                        plus(when(var(dist), second), var(dist)), _) )).
check(repair, ( clock([dist:base],
                      plus(current(when(var(dist), second)), var(dist)), base) )).
check(merge_totals, ( clock([fast:on(base, c), slow:on(base, not(c))],
                            merge(c, var(fast), var(slow)), base) )).

go :- forall(check(N, G),
             ( catch(G, E, (print_message(error, E), fail))
             -> format("PASS  ~w~n", [N]) ; format("fail  ~w~n", [N]) )).
