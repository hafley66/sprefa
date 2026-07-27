% retention.pl : max nested pre depth = the window bound. The runtime keeps
% exactly this many ticks of history per stream and GC's the rest; it is why
% Lustre's memory footprint is known at compile time. pre(E) = LAG(e, depth).
% Run: swipl -q -l retention.pl -g go -g halt

depth(const(_), 0).
depth(var(_), 0).
depth(pre(E), D)      :- depth(E, DE), D is DE + 1.
depth(plus(A, B), D)  :- depth(A, DA), depth(B, DB), D is max(DA, DB).
depth(arrow(A, B), D) :- depth(A, DA), depth(B, DB), D is max(DA, DB).
depth(fby(A, B), D)   :- depth(A, DA), depth(B, DB), DB1 is DB + 1, D is max(DA, DB1).
depth(when(E, _), D)  :- depth(E, D).
depth(current(E), D)  :- depth(E, D).

check(nested,  ( depth(plus(pre(pre(var(x))), var(x)), 2) )).
check(fby_one, ( depth(fby(const(0), plus(var(n), const(1))), 1) )).

go :- forall(check(N, G),
             ( catch(G, E, (print_message(error, E), fail))
             -> format("PASS  ~w~n", [N]) ; format("fail  ~w~n", [N]) )).
