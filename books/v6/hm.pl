% hm.pl — everything needed to hand-derive (and machine-check) the twice trace.
%
% Run:      swipl -q -l books/v6/hm.pl
% Query:    ?- infer(twice, T).
% Watch it: ?- trace, infer(twice, T).      (creep with Enter, `a` to abort)
% Quit:     ?- halt.
%
% Reading rules:
%   lowercase = data (atoms/functors).  Uppercase = holes (logic variables).
%   Every clause USE stamps fresh copies of its variables (per-call locals).
%   `X : T` is just an infix pair term.  `[H|T]` is just cons.
%   `A -> B` here is DATA (the arrow type), not prolog's if-then-else;
%   it only ever appears inside term arguments, where that reading is safe.

% ─────────────────────────────────────────────────────────────────────────────
% Lambda AST encoding (plain terms, nothing evaluates):
%   var(x)          a variable reference          x
%   lam(x, Body)    λx. Body                      (x) => Body      one param, always
%   app(F, A)       F A                           F(A)             one arg, always
%   lit(3)          a number literal              3
%
% You already write this language daily — it is the AST of arrow functions:
%   identity = x => x                    lam(x, var(x))
%   apply    = f => x => f(x)            multi-arg is NESTED lams (currying)
%   twice    = f => x => f(f(x))
%   konst    = x => y => x
%   selfapp  = x => x(x)
% Three node kinds, nothing else: no if, no numbers, no let. The 1930s result
% is that this suffices for all computation (numbers = Church numerals like
% twice; recursion = Y). Smallest language that is still everything.
%
% What HM is, in those terms: the algorithm that hovers over
%   f => x => f(f(x))
% and prints the type WITH ZERO ANNOTATIONS. TypeScript errors here
% ("f implicitly has an any type"): TS does local inference and needs
% parameter annotations to seed it. HM mints a hole per parameter, walks the
% body turning every call into an equation (callee domain = argument type),
% and unification precipitates the answer. Why twice is (A->A)->A->A and not
% (A->B)->...: inside f(f(x)) the inner call's OUTPUT feeds the outer call's
% INPUT, so f's domain and codomain get soldered to the same hole.
%
% First-order note: in prolog a variable cannot sit in functor position —
% X(a) is a syntax error. Higher-orderness goes through call/N (which also
% curries: call(padd(A), B, C) appends args) or =.. ("univ", functor⇄list).
% Ladder: datalog (no terms) < prolog (first-order) < λProlog (higher-order
% unification, undecidable in general). Each rung trades a guarantee away.
% ─────────────────────────────────────────────────────────────────────────────

% ── The type checker. Four clauses. This is the whole thing. ────────────────

type(_Env, lit(N), int) :-
    number(N).

type(Env, var(X), T) :-                 % lookup = unification via member
    member(X:T, Env).

type(Env, lam(X, Body), TX -> TB) :-    % TX minted fresh per use: "x has
    type([X:TX | Env], Body, TB).       %  SOME type, constrained later"

type(Env, app(F, A), T) :-              % TA appears twice: THAT sharing is
    type(Env, F, TA -> T),              %  the "argument matches parameter"
    type(Env, A, TA).                   %  constraint. No check is written.

  

% ── Named example terms ─────────────────────────────────────────────────────

example(identity, lam(x, var(x))).
example(apply,    lam(f, lam(x, app(var(f), var(x))))).
example(twice,    lam(f, lam(x, app(var(f), app(var(f), var(x)))))).
example(compose,  lam(f, lam(g, lam(x, app(var(f), app(var(g), var(x))))))).
example(const,    lam(x, lam(y, var(x)))).
% self-application: the one HM rejects (occurs check → infinite type).
example(selfapp,  lam(x, app(var(x), var(x)))).

infer(Name, T) :-
    example(Name, Term),
    type([], Term, T).

% Expected answers (derive them by hand first, then check):
%   identity : A -> A
%   apply    : (A -> B) -> A -> B
%   twice    : (A -> A) -> A -> A        ← the collision forces both arrows equal
%   compose  : (B -> C) -> (A -> B) -> A -> C
%   const    : A -> B -> A
%   selfapp  : ?- infer(selfapp, T).  succeeds with a CYCLIC term unless you
%              use unify_with_occurs_check — swipl skips the occurs check by
%              default for speed. Try:
%              ?- set_prolog_flag(occurs_check, true), infer(selfapp, T).
%              → fails. That failure IS "λx. x x has no simple type."

% ─────────────────────────────────────────────────────────────────────────────
% Warmups from the lessons, runnable.
% ─────────────────────────────────────────────────────────────────────────────

% pairs + cons refresher:
%   ?- Name:Type = x:int.
%   ?- [H|T] = [1,2,3].

% sum with real arithmetic (`is` evaluates; `=` never does):
sum([], 0).
sum([H|T], S) :- sum(T, ST), S is ST + H.

% Peano: numbers as pumped constructors. No `is` anywhere.
% zero, s(zero), s(s(zero)) ...
padd(zero, N, N).
padd(s(M), N, s(S)) :- padd(M, N, S).

% multiplication: s(M)*N = N + M*N
pmul(zero, _, zero).
pmul(s(M), N, P) :- pmul(M, N, MN), padd(N, MN, P).

% Run these forwards, backwards, and with all holes:
%   ?- padd(s(s(zero)), s(zero), S).        % 2+1
%   ?- padd(X, s(zero), s(s(s(zero)))).     % X+1=3, subtraction for free
%   ?- padd(X, Y, s(s(zero))).              % all partitions of 2
%   ?- pmul(s(s(zero)), s(s(s(zero))), P).  % 2*3

% ─────────────────────────────────────────────────────────────────────────────
% The clock-calculus reuse: same solver, different term grammar.
%   clock ::= base | on(Clock, BoolStreamName)
% `+` demands both sides share a clock; when/current change it.
% ─────────────────────────────────────────────────────────────────────────────

clock(_Env, lit(_), _AnyClock).                 % constants live on every clock
clock(Env, var(X), C)        :- member(X:C, Env).
clock(Env, plus(A, B), C)    :- clock(Env, A, C), clock(Env, B, C).   % SAME C
clock(Env, when(A, S), on(C, S)) :- clock(Env, A, C).                 % thin time
clock(Env, current(A), C)    :- clock(Env, A, on(C, _)).              % stretch back

% The lesson example: slow = dist when second; then slow + dist is a clash.
%   ?- clock([dist:base, second:base], plus(when(var(dist), second), var(dist)), C).
%   → false.        (unify on(base,second) = base: constructor clash)
%   ?- clock([dist:base], plus(current(when(var(dist), second)), var(dist)), C).
%   → C = base.     (the explicit repair)
