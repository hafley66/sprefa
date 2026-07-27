% enum_match.pl : rust enums, match, and effect envelopes on the HM kernel.
%
% Run:     swipl -q -l books/v6/enum_match.pl
% Score:   ?- go.
%
% The rust surface and its encoding here:
%
%   enum Option<A> { Some(A), None }        con(some, [A], option(A)).
%                                           con(none, [],  option(_)).
%   enum Result<A, E> { Ok(A), Err(E) }     con(ok,  [A], result(A, _)).
%                                           con(err, [E], result(_, E)).
%
%   match x { Some(v) => v, None => 0 }     match(var(x), [ arm(pat(some,[v]), var(v))
%                                                         , arm(pat(none,[]),  lit(0)) ])
%
% Generics cost ZERO code: every use of a con/3 clause stamps fresh copies of
% its type holes, which is exactly rust's per-use instantiation of <A>.
% Exhaustiveness is a sorted-list subtraction. And an EFFECT declares an
% envelope type for what comes out of it; a match on an effect result that
% forgets an arm (the error channel) fails to type. That is `must handle
% Err` enforced by the same unification loop as everything else.

:- use_module(library(lists)).
:- use_module(library(ordsets)).

% ── constructor table (the enum declarations) ───────────────────────────────

con(some, [A],       option(A)).
con(none, [],        option(_)).
con(ok,   [A],       result(A, _)).
con(err,  [E],       result(_, E)).
con(circle, [float],        shape).
con(rect,   [float, float], shape).

% ── effect envelopes: what comes OUT of a yield point, as a type ────────────

effect(http, request, result(int, int)).    % payload int, error code int

% ── the checker: hm.pl's four clauses plus three ────────────────────────────

type(_Env, lit(N), int) :- number(N).
type(Env, var(X), T) :- member(X:T, Env).
type(Env, lam(X, Body), TX -> TB) :- type([X:TX | Env], Body, TB).
type(Env, app(F, A), T) :- type(Env, F, TA -> T), type(Env, A, TA).

type(Env, cons(Name, Args), T) :-           % construction: Some(x)
    con(Name, ArgTypes, T),                 % fresh per use = generics
    args_type(Env, Args, ArgTypes).

type(Env, do_effect(Name, Arg), Envelope) :-   % calling an effect returns
    effect(Name, ArgType, Envelope),           % its envelope, nothing else
    type(Env, Arg, ArgType).

type(Env, match(Scrut, Arms), T) :-
    type(Env, Scrut, ScrutType),
    maplist(arm_type(Env, ScrutType, T), Arms),   % all arms one result type
    exhaustive(ScrutType, Arms).

args_type(_, [], []).
args_type(Env, [Arg | Args], [Ty | Tys]) :-
    type(Env, Arg, Ty),
    args_type(Env, Args, Tys).

arm_type(Env, ScrutType, T, arm(pat(Name, Vars), Body)) :-
    con(Name, ArgTypes, ScrutType),         % pattern picks the constructor,
    extend(Vars, ArgTypes, Env, ArmEnv),    % binders enter env at arg types
    type(ArmEnv, Body, T).

extend([], [], Env, Env).
extend([Var | Vars], [Ty | Tys], Env, EnvOut) :-
    extend(Vars, Tys, [Var:Ty | Env], EnvOut).

% every constructor of the scrutinee's enum must appear in some arm
exhaustive(ScrutType, Arms) :-
    findall(Name, ( con(Name, _, ConType), \+ ConType \= ScrutType ), Needed0),
    sort(Needed0, Needed),
    findall(Name, member(arm(pat(Name, _), _), Arms), Got0),
    sort(Got0, Got),
    ord_subtract(Needed, Got, []).

% ── examples ────────────────────────────────────────────────────────────────

% |x| match x { Some(v) => v, None => 0 }
example(unwrap_or_zero,
  lam(x, match(var(x),
    [ arm(pat(some, [v]), var(v))
    , arm(pat(none, []),  lit(0))
    ]))).

% |x| Some(x)
example(wrap, lam(x, cons(some, [var(x)]))).

% |r| match http(r) { Ok(n) => n, Err(c) => c }
example(handle_http,
  lam(r, match(do_effect(http, var(r)),
    [ arm(pat(ok,  [n]), var(n))
    , arm(pat(err, [c]), var(c))
    ]))).

% |r| match http(r) { Ok(n) => n }        <- forgot the error channel
example(forgot_err,
  lam(r, match(do_effect(http, var(r)),
    [ arm(pat(ok, [n]), var(n))
    ]))).

% ── grader ──────────────────────────────────────────────────────────────────

check(unwrap_typed,   ( example(unwrap_or_zero, Term),
                        type([], Term, Ty),
                        Ty == (option(int) -> int) )).
check(wrap_generic,   ( example(wrap, Term),
                        type([], Term, TIn -> option(TOut)),
                        TIn == TOut, var(TIn) )).       % stayed polymorphic
check(effect_handled, ( example(handle_http, Term),
                        type([], Term, Ty),
                        Ty == (request -> int) )).
check(missing_err,    ( example(forgot_err, Term),
                        \+ type([], Term, _) )).        % rejected statically

go :-
    forall(check(Name, Goal),
           ( catch(Goal, Error, (print_message(error, Error), fail))
           -> format("PASS  ~w~n", [Name])
           ;  format("fail  ~w~n", [Name]) )).
