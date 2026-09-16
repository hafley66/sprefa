% unify_hm.pl : Hindley-Milner type inference. The whole algorithm is 4 clauses;
% unification is prolog's own. Run: swipl -q -l unify_hm.pl -g go -g halt

:- use_module(library(lists)).

type(_Env, lit(N), int) :- number(N).
type(Env, var(X), T) :- member(X:T, Env).
type(Env, lam(X, Body), TX -> TB) :- type([X:TX | Env], Body, TB).
type(Env, app(F, A), T) :- type(Env, F, TA -> T), type(Env, A, TA).

example(apply,   lam(f, lam(x, app(var(f), var(x))))).
example(twice,   lam(f, lam(x, app(var(f), app(var(f), var(x)))))).
example(selfapp, lam(x, app(var(x), var(x)))).

check(apply, ( example(apply, T), type([], T, Ty), Ty =@= ((A -> B) -> A -> B) )).
check(twice, ( example(twice, T), type([], T, Ty), Ty =@= ((A -> A) -> A -> A) )).
check(selfapp_rejected,
      ( example(selfapp, T),
        set_prolog_flag(occurs_check, true),
        ( type([], T, _) -> Verdict = typed ; Verdict = rejected ),
        set_prolog_flag(occurs_check, false),
        Verdict == rejected )).

go :- forall(check(N, G),
             ( catch(G, E, (print_message(error, E), fail))
             -> format("PASS  ~w~n", [N]) ; format("fail  ~w~n", [N]) )).
