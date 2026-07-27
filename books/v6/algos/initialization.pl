% initialization.pl : Lustre initialization analysis. Abstract interpretation
% over {d, u}: is the stream defined at its first instant? pre poisons,
% arrow/fby cure, current is undefined until its first true sample.
% Run: swipl -q -l initialization.pl -g go -g halt

:- use_module(library(lists)).

init(_Env, const(_), d).
init(Env, var(X), I)      :- member(X:I, Env).
init(_Env, pre(_), u).
init(Env, arrow(A, _), I) :- init(Env, A, I).
init(Env, fby(A, _), I)   :- init(Env, A, I).
init(Env, plus(A, B), I)  :- init(Env, A, IA), init(Env, B, IB), ijoin(IA, IB, I).
init(Env, when(E, _), I)  :- init(Env, E, I).
init(_Env, current(_), u).

ijoin(d, d, d).
ijoin(u, _, u).
ijoin(d, u, u).

check(scan_ok,  ( init([inp:d], arrow(const(0), plus(pre(var(acc)), var(inp))), d) )).
check(bare_pre, ( init([n:d], plus(pre(var(n)), const(1)), u) )).
check(current_hole, ( init([e:d], current(when(var(e), c)), u) )).

go :- forall(check(N, G),
             ( catch(G, E, (print_message(error, E), fail))
             -> format("PASS  ~w~n", [N]) ; format("fail  ~w~n", [N]) )).
