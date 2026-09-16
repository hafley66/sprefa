% lustre.pl — the Lustre front-end analyses, each as a tiny prolog program.
%
% Run:     swipl -q -l books/v6/lustre.pl
% Score:   ?- go.
% Reload:  ?- make.
%
% AST encoding (plain terms, nothing evaluates):
%   const(N)          constant stream        0, 0, 0, ...
%   var(X)            read a named stream
%   plus(A, B)        pointwise op           (stands in for every binop)
%   pre(E)            lag 1                  LAG(e, 1) OVER (ORDER BY tick)
%   arrow(A, B)       A -> B                 tick 0 from A, later from B (COALESCE)
%   fby(A, B)         A fby B  =  arrow(A, pre(B))
%   when(E, S)        thin E to instants where boolean stream S is true
%   current(E)        stretch back by sample-and-hold   (old lustre)
%   merge(S, T, F)    clock pattern-match: T on S, F on not S   (new lustre)
%
% A node body = list of eq(Name, Expr).
%
% FOUR analyses, THREE solver species:
%   clock/3   unification       same solver as hm.pl's type/3, new term grammar
%   depth/2   syntactic fold    retention bound: how many ticks to keep (LAG max)
%   init/3    lattice fold      abstract interpretation over {d, u}
%   causal/1  graph fixpoint    the datalog-shaped one — note the :- table

% ─────────────────────────────────────────────────────────────────────────────
% 1. CLOCK INFERENCE — unification. A clock is base or on(Clock, StreamName).
%    plus demands both sides share ONE clock hole; when thins; current/merge
%    stretch back. Same 4-clause shape as the type checker.
% ─────────────────────────────────────────────────────────────────────────────

clock(_Env, const(_), _AnyClock).                 % constants live on every clock
clock(Env, var(X), C)         :- member(X:C, Env).
clock(Env, plus(A, B), C)     :- clock(Env, A, C), clock(Env, B, C).
clock(Env, pre(E), C)         :- clock(Env, E, C).
clock(Env, arrow(A, B), C)    :- clock(Env, A, C), clock(Env, B, C).
clock(Env, fby(A, B), C)      :- clock(Env, A, C), clock(Env, B, C).
clock(Env, when(E, S), on(C, S))  :- clock(Env, E, C).
clock(Env, current(E), C)     :- clock(Env, E, on(C, _)).
clock(Env, merge(S, T, F), C) :-                  % the current-killer: both
    clock(Env, T, on(C, S)),                      % branches on complementary
    clock(Env, F, on(C, not(S))).                 % subclocks, result on base

% ─────────────────────────────────────────────────────────────────────────────
% 2. RETENTION — max nested pre depth. This number IS the window bound:
%    the runtime keeps exactly depth ticks of history per stream, GC's the rest.
%    (Lustre's compile-time-bounded-memory guarantee is this fold.)
% ─────────────────────────────────────────────────────────────────────────────

depth(const(_), 0).
depth(var(_), 0).
depth(pre(E), D)      :- depth(E, DE), D is DE + 1.
depth(plus(A, B), D)  :- depth(A, DA), depth(B, DB), D is max(DA, DB).
depth(arrow(A, B), D) :- depth(A, DA), depth(B, DB), D is max(DA, DB).
depth(fby(A, B), D)   :- depth(A, DA), depth(B, DB), DB1 is DB + 1, D is max(DA, DB1).
depth(when(E, _), D)  :- depth(E, D).
depth(current(E), D)  :- depth(E, D).
depth(merge(_, T, F), D) :- depth(T, DT), depth(F, DF), D is max(DT, DF).

% ─────────────────────────────────────────────────────────────────────────────
% 3. INITIALIZATION — is the stream defined at its first instant?
%    Two abstract values: d (defined from tick 0), u (undefined at tick 0).
%    pre poisons; arrow/fby cure (tick 0 comes from the left arg);
%    current is u (before the first true sample there is nothing to hold —
%    the init bug that motivated merge).
% ─────────────────────────────────────────────────────────────────────────────

init(_Env, const(_), d).
init(Env, var(X), I)      :- member(X:I, Env).
init(_Env, pre(_), u).
init(Env, arrow(A, _), I) :- init(Env, A, I).
init(Env, fby(A, _), I)   :- init(Env, A, I).
init(Env, plus(A, B), I)  :- init(Env, A, IA), init(Env, B, IB), ijoin(IA, IB, I).
init(Env, when(E, _), I)  :- init(Env, E, I).
init(_Env, current(_), u).
init(Env, merge(_, T, F), I) :- init(Env, T, IT), init(Env, F, IF), ijoin(IT, IF, I).

ijoin(d, d, d).                                   % defined only if BOTH are
ijoin(u, _, u).
ijoin(d, u, u).

% ─────────────────────────────────────────────────────────────────────────────
% 4. CAUSALITY — build the instantaneous-dependency graph, reject cycles.
%    idep(Expr, X): Expr reads stream X AT THE SAME TICK. pre cuts the edge
%    (that is the whole point of a register); fby's second arg is under pre.
%    reach is transitive closure — the graph algo prolog loops on without
%    tabling. `:- table` = SLG = the datalog move, from inside prolog.
% ─────────────────────────────────────────────────────────────────────────────

idep(var(X), X).
idep(plus(A, B), X)    :- ( idep(A, X) ; idep(B, X) ).
idep(arrow(A, B), X)   :- ( idep(A, X) ; idep(B, X) ).
idep(fby(A, _), X)     :- idep(A, X).             % second arg is under pre
idep(when(E, S), X)    :- ( idep(E, X) ; X = S ).
idep(current(E), X)    :- idep(E, X).
idep(merge(S, T, F), X) :- ( X = S ; idep(T, X) ; idep(F, X) ).
                                                  % const, pre: no clause = no edge

:- table reach/3.

edge(Eqs, From, Dep) :- member(eq(From, Expr), Eqs), idep(Expr, Dep).
reach(Eqs, A, B)     :- edge(Eqs, A, B).
reach(Eqs, A, B)     :- edge(Eqs, A, Mid), reach(Eqs, Mid, B).

causal(Eqs) :- \+ ( member(eq(X, _), Eqs), reach(Eqs, X, X) ).

% ─────────────────────────────────────────────────────────────────────────────
% Named example nodes.
% ─────────────────────────────────────────────────────────────────────────────

% n = 0 fby (n + 1)          the counter; legal BECAUSE fby hides a pre
node(counter, [eq(n, fby(const(0), plus(var(n), const(1))))]).

% x = x + 1                  instantaneous self-loop; must be rejected
node(broken,  [eq(x, plus(var(x), const(1)))]).

% acc = 0 -> (pre acc + inp) running sum, the scan
node(running_sum, [eq(acc, arrow(const(0), plus(pre(var(acc)), var(inp))))]).

% ─────────────────────────────────────────────────────────────────────────────
% The grader.
% ─────────────────────────────────────────────────────────────────────────────

check(clock_shared,  ( clock([dist:base], plus(var(dist), const(0)), base) )).
check(clock_clash,   ( \+ clock([dist:base, second:base],
                                plus(when(var(dist), second), var(dist)), _) )).
check(clock_merge,   ( clock([fast:on(base, c), slow:on(base, not(c))],
                             merge(c, var(fast), var(slow)), base) )).
check(depth_nested,  ( depth(plus(pre(pre(var(x))), var(x)), 2) )).
check(depth_fby,     ( node(counter, [eq(_, E)]), depth(E, 1) )).
check(init_scan_ok,  ( init([inp:d], arrow(const(0), plus(pre(var(acc)), var(inp))), d) )).
check(init_bare_pre, ( init([n:d], plus(pre(var(n)), const(1)), u) )).
check(init_current,  ( init([e:d], current(when(var(e), c)), u) )).
check(causal_counter,( node(counter, Eqs), causal(Eqs) )).
check(causal_broken, ( node(broken, Eqs), \+ causal(Eqs) )).

go :-
    forall(check(Name, Goal),
           ( catch(Goal, _, fail)
           -> format("PASS  ~w~n", [Name])
           ;  format("fail  ~w~n", [Name]) )).
