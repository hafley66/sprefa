% drills.pl — prolog reps before the HM trace. No lambda calculus in here.
%
% Run:     swipl -q -l books/v6/drills.pl
% Score:   ?- go.                 (every drill prints PASS or fail)
% Reload:  ?- make.               (after editing this file, no restart needed)
% Answers: ?- member(X, [a,b,c]). then press `;` for the NEXT solution, Enter to stop.
%
% Loop: predict → run → fill a TODO → make. → go. Solutions at the bottom,
% commented out. Peeking before a real attempt defeats the entire purpose.

% ═════════════════════════════════════════════════════════════════════════════
% PART 0 — prediction drills. Write your guess down FIRST, then paste the query.
% Each is about one fact of unification.
% ═════════════════════════════════════════════════════════════════════════════
%
%  p1.  ?- X = 3.
%  p2.  ?- 3 = X.                          (direction matters? does it?)
%  p3.  ?- point(X, 4) = point(3, Y).
%  p4.  ?- f(X, X) = f(1, 2).
%  p5.  ?- f(X, Y) = g(X, Y).
%  p6.  ?- A = B, B = C, C = 7.            (wire soldering, three deep)
%  p7.  ?- k:V = k:int.
%  p8.  ?- [H|T] = [a].
%  p9.  ?- [H|T] = [].
%  p10. ?- [A, B|R] = [1, 2].
%  p11. ?- X = 1 + 2.                      (is X 3? why or why not?)
%  p12. ?- X is 1 + 2.
%  p13. ?- member(N:V, [a:1, b:2]), V > 1. (what is N? how many answers?)
%  p14. ?- \+ member(z, [a, b]).           (\+ is "cannot prove" — negation)

% ═════════════════════════════════════════════════════════════════════════════
% PART 1 — write these. Each has a stub marked TODO; the checks below grade you.
% dynamic = "calling it while undefined just fails instead of erroring".
% ═════════════════════════════════════════════════════════════════════════════

:- dynamic mylen/2, mylast/2, lookup/3, double_all/2, pless/2, myrange/3.

% d1. mylen(List, N): length, real arithmetic (`is`). Two clauses.
% TODO
mylen([], 0).                            % "len of [] IS 0" — no body needed
mylen([_|T], N) :- mylen(T, NT), N is NT + 1.
% mylen(List, N) :- 

% d2. mylast(List, X): last element. Two clauses, no arithmetic.
%     Hint: the base case is a ONE-element list, not [].
% TODO
mylast([H], H).
mylast([_|T], Ret) :- mylast(T, Ret).

% d3. lookup(Key, Env, Val): first match wins, Env is [k1:v1, k2:v2, ...].
%     This is EXACTLY the env lookup the type checker does. Two clauses.
%     Write it without member/2.
% TODO
lookup(Key, [Key:Val|_], Val).
lookup(Key, [_|T], Val):- lookup(Key, T, Val).

% d4. double_all(Ns, Ds): [1,2,3] -> [2,4,6]. Two clauses.
double_all([H], Ds) :- 

% d5. pless(A, B): Peano less-than. zero < s(anything); peel both otherwise.
%     Two clauses, no arithmetic, no negation.
% TODO

% d6. myrange(Lo, Hi, List): myrange(2, 5, [2,3,4,5]). Two clauses.
%     Careful: the base case needs Lo and Hi EQUAL, and clause two needs
%     Lo < Hi as a guard plus L1 is Lo + 1.
% TODO

% ═════════════════════════════════════════════════════════════════════════════
% The grader. Leave alone.
% ═════════════════════════════════════════════════════════════════════════════

check(d1_mylen,      ( mylen([a,b,c], 3), mylen([], 0) )).
check(d2_mylast,     ( mylast([1,2,3], 3), \+ mylast([], _) )).
check(d3_lookup,     ( lookup(x, [y:bool, x:int], int),
                       lookup(x, [x:first, x:second], first),
                       \+ lookup(z, [x:int], _) )).
check(d4_double_all, ( double_all([1,2,3], [2,4,6]), double_all([], []) )).
check(d5_pless,      ( pless(zero, s(zero)),
                       pless(s(zero), s(s(zero))),
                       \+ pless(s(zero), s(zero)),
                       \+ pless(s(zero), zero) )).
check(d6_myrange,    ( myrange(2, 5, [2,3,4,5]), myrange(7, 7, [7]) )).

go :-
    forall(check(Name, Goal),
           ( catch(Goal, _, fail)
           -> format("PASS  ~w~n", [Name])
           ;  format("fail  ~w~n", [Name]) )).

% ═════════════════════════════════════════════════════════════════════════════
% PART 2 — backtracking feel. No writing; run and watch, press `;` a lot.
% ═════════════════════════════════════════════════════════════════════════════
%
%  b1. ?- member(X, [a,b,c]).                     3 answers
%  b2. ?- member(X, [a,b]), member(Y, [1,2]).     4 answers — nested loops ARE
%                                                  backtracking; this is the join
%  b3. ?- append(A, B, [1,2]).                    all splits of a list
%  b4. ?- lookup(x, [x:first, x:second], V).      press ; — why 2 answers?
%                                                  (and why does the checker
%                                                   only bless `first`?)
%
% b2 is worth a pause: two goals sharing no variables = cross join; the moment
% they share one, it becomes an inner join. Datalog's SELECT..JOIN is this,
% minus the one-at-a-time enumeration.

% ═════════════════════════════════════════════════════════════════════════════
% SOLUTIONS — genuinely try first. `make.` then `go.` after each attempt.
% ═════════════════════════════════════════════════════════════════════════════
%
% mylen([], 0).
% mylen([_|T], N) :- mylen(T, NT), N is NT + 1.
%
% mylast([X], X).
% mylast([_|T], X) :- mylast(T, X).
%
% lookup(K, [K:V|_], V).
% lookup(K, [_|Rest], V) :- lookup(K, Rest, V).
%   (subtle: clause 1 head does the match; if you also want first-match-ONLY
%    on backtracking you'd need a cut or K \= K2 in clause 2 — the checker's
%    lookup(x, [x:first, x:second], first) passes because clause order tries
%    `first` first; pressing ; in b4 shows `second` is also derivable.)
%
% double_all([], []).
% double_all([N|Ns], [D|Ds]) :- D is N * 2, double_all(Ns, Ds).
%
% pless(zero, s(_)).
% pless(s(A), s(B)) :- pless(A, B).
%
% myrange(H, H, [H]).
% myrange(L, H, [L|T]) :- L < H, L1 is L + 1, myrange(L1, H, T).
