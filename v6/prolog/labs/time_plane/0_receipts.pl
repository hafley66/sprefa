% 0_receipts.pl : time-plane unification lab, the reference-engine half.
%
%   swipl -q -l v6/prolog/labs/time_plane/0_receipts.pl -g go -g halt
%
% Every receipt runs the REAL oracle (conformance/engine.pl), unmodified. The
% receipts here establish the BASELINE the unification is graded against; the
% prototype half (retention_minus.patch) is applied and measured by
% receipts.sh, not from inside this file.
%
% Numbering is T<n> so it does not collide with the rel-as-stream lab's R<n>.

:- use_module('../../conformance/engine').

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

:- dynamic failures/1.
failures(0).

bump :- retract(failures(N)), M is N + 1, assertz(failures(M)).

check(Label, Goal) :-
    (   catch(Goal, Error, ( format("FAIL ~w~n  raised ~q~n", [Label, Error]),
                             bump, fail ))
    ->  format("PASS ~w~n", [Label])
    ;   format("FAIL ~w~n", [Label]), bump
    ).

% Run a program, keep the final rows and the per-tick deltas.
run(Prog, Initial, Schedule, Final, Deltas) :-
    engine:run_program(Prog, Initial, Schedule, Final, Deltas).

% Run a program expected to throw Term.
throws(Prog, Initial, Schedule, Term) :-
    catch(( engine:run_program(Prog, Initial, Schedule, _, _), fail ),
          Thrown, true),
    Thrown == Term.

count_rows(Ref, Final, Count) :-
    engine:rel_rows(Ref, Final, Rows), length(Rows, Count).

% ═══ Q1: the identity split ═════════════════════════════════════════════════
%
% The header asks whether a seq column EXCLUDED from identity keeps a bare set
% rel zero-delta on identical re-arrival while a sugared log rel stacks. T1-T3
% answer the prior question the sugar hypothesis depends on: what does the
% engine let a RULE see of a repeated arrival.

% T1. A set rel absorbs an identical second arrival as a NO-OP. The edge rule
% fires ONCE. This is the fact the whole sugar hypothesis runs into: the second
% occurrence does not exist, so no expansion written in rules can stamp it.
t1 :- run(prog([ col_type(ev/1, name, text),
                 col_type(fired/1, name, text),
                 kind(fired/1, log), keep(fired/1, all) ],
               [ (fired(Name) <+ ev(Name)) ]),
          [], [[ +ev(a), +ev(a) ]], Final, _),
      count_rows(fired/1, Final, 1).

% T2. The same two arrivals into a LOG rel fire the rule TWICE.
t2 :- run(prog([ col_type(ev/1, name, text), kind(ev/1, log), keep(ev/1, all),
                 col_type(fired/1, name, text),
                 kind(fired/1, log), keep(fired/1, all) ],
               [ (fired(Name) <+ ev(Name)) ]),
          [], [[ +ev(a), +ev(a) ]], Final, _),
      count_rows(fired/1, Final, 2).

% T3. A SET rel whose two arrivals DIFFER in one column fires twice as well.
% So multiplicity is already free on the set plane once the rows differ: the
% log kind is not what buys duplicate delivery, DISTINCTNESS is. What the log
% kind buys is the engine filling that distinctness when the world does not.
t3 :- run(prog([ col_type(ev/2, name, text), col_type(ev/2, seq, int),
                 col_type(fired/1, name, text),
                 kind(fired/1, log), keep(fired/1, all) ],
               [ (fired(Name) <+ ev(Name, _Seq)) ]),
          [], [[ +ev(a, 1), +ev(a, 2) ]], Final, _),
      count_rows(fired/1, Final, 2).

% ═══ Q3/Q4 baselines: what retention does today ═════════════════════════════

% T4. keep(count(1)) prunes -- the final store holds one row -- and the tick
% log reports NO minus. This is the retention-grading gap, stated as a receipt.
t4 :- run(prog([ col_type(ev/1, name, text), kind(ev/1, log), keep(ev/1, count(1)) ],
               []),
          [], [[ +ev(a) ], [ +ev(b) ], [ +ev(c) ]], Final, Deltas),
      count_rows(ev/1, Final, 1),
      engine:rel_deltas(ev/1, Deltas, PerTick),
      PerTick == [[ +ev(a) ], [ +ev(b) ], [ +ev(c) ]].

% T5. finalize/1 over that same pruning log rel fires NOTHING, with no refusal.
% (rel-as-stream R12; re-run here because the prototype flips exactly this.)
t5 :- run(prog([ col_type(ev/1, name, text), kind(ev/1, log), keep(ev/1, count(1)),
                 col_type(gone/1, name, text), kind(gone/1, log), keep(gone/1, all) ],
               [ (gone(Name) <+ finalize(ev(Name))) ]),
          [], [[ +ev(a) ], [ +ev(b) ], [ +ev(c) ]], Final, _),
      count_rows(gone/1, Final, 0).

% ═══ Q2: the five cross-plane refusals, as they stand ═══════════════════════

% T6. R7 itself: an occurrence cannot un-happen.
t6 :- throws(prog([ col_type(ev/1, name, text), kind(ev/1, log), keep(ev/1, all) ], []),
             [], [[ +ev(a) ], [ -ev(a) ]], retract_from_log(ev/1)).

% T7. A rel whose rows are COMPUTED has no delta channel to accumulate on.
t7 :- throws(prog([ col_type(src/1, name, text),
                    col_type(derived/1, name, text),
                    kind(derived/1, log), keep(derived/1, all) ],
                  [ (derived(Name) <- src(Name)) ]),
             [], [], log_on_level_headed_rel(derived/1)).

% T8. Retention is meaningless off the occurrence plane.
t8 :- throws(prog([ col_type(plain/1, name, text), keep(plain/1, all) ], []),
             [], [], keep_on_non_log_rel(plain/1)).

% T9. A log rel must say how much history it keeps.
t9 :- throws(prog([ col_type(ev/1, name, text), kind(ev/1, log) ], []),
             [], [], missing_retention(ev/1)).

% T10. A key is a membership invariant; occurrences have no membership.
t10 :- throws(prog([ col_type(ev/2, id, int), col_type(ev/2, name, text),
                     kind(ev/2, log), keep(ev/2, all), keyed(ev/2, [1]) ], []),
              [], [], keyed_log_rel(ev/2)).

% ═══ slot_seq_scope: the two doors already disagree, measured ═══════════════
%
% The header names this slot and says the stream lab left it ungraded because
% unobservable. Both halves below are observable the moment a seq becomes a
% COLUMN, which is exactly what unification proposes.

% T11. The oracle's arrival counter is GLOBAL across rels and counts SET
% arrivals too: a set row landing between two log rows consumes an ordinal.
% Surfaced as a column, this log rel would carry ordinals 1 and 3 -- a gap
% minted by an unrelated relation. SQLite's per-table rowid gives 1 and 2.
t11 :- engine:absorb_arrivals(
           prog([ col_type(mid/1, name, text),
                  col_type(lg/1, name, text), kind(lg/1, log), keep(lg/1, all) ], []),
           7, [ +lg(one), +mid(m), +lg(two) ], [], 1, Store, _, _),
       memberchk(lrow(st(7, 1), lg(one)), Store),
       memberchk(lrow(st(7, 3), lg(two)), Store).

% T12. The same counter is shared BETWEEN log rels, so per-rel numbering and
% the oracle's numbering disagree on every interleaved tick.
t12 :- engine:absorb_arrivals(
           prog([ col_type(zeta/1, name, text), kind(zeta/1, log), keep(zeta/1, all),
                  col_type(alpha/1, name, text), kind(alpha/1, log), keep(alpha/1, all) ], []),
           7, [ +zeta(x), +alpha(y), +zeta(z) ], [], 1, Store, _, _),
       memberchk(lrow(st(7, 1), zeta(x)), Store),
       memberchk(lrow(st(7, 2), alpha(y)), Store),
       memberchk(lrow(st(7, 3), zeta(z)), Store).

% ═══ Q2 residual: what a seq column would do to the world boundary ══════════

% T13. A log rel and a set rel carrying the SAME columns are distinguishable
% only by the decl: the world sends the same wire row either way. So a seq
% column that the WORLD must supply changes the arrival contract, and a seq
% column the ENGINE supplies keeps an engine branch. T13 pins the first half:
% the identical schedule produces one row on the set plane and two on the log
% plane, with no difference in what the world sent.
t13 :- Schedule = [[ +ev(a), +ev(a) ]],
       run(prog([ col_type(ev/1, name, text) ], []), [], Schedule, SetFinal, _),
       run(prog([ col_type(ev/1, name, text), kind(ev/1, log), keep(ev/1, all) ], []),
           [], Schedule, LogFinal, _),
       count_rows(ev/1, SetFinal, 1),
       count_rows(ev/1, LogFinal, 2).

% ═══ H2: the metadata plane, written as an ordinary program ════════════════
%
% The header proposes created_at_tick / updated_at_tick as an auto METADATA
% PLANE. T14/T15 measure what a program can already do with shipped constructs
% (now/1 is the emitted tick counter; pre/1 reads the evolving pre-state).

% T14. The NAIVE one-rule spelling does NOT pin a birth tick. The rel is keyed,
% so the second arrival for key 1 REPLACES, and the tick column follows the
% write -- a column named created_at_tick that actually holds updated_at
% semantics. Named here because it is the trap the sugar would exist to close.
t14 :- run(prog([ col_type(arrive/2, id, int), col_type(arrive/2, payload, text),
                  kind(arrive/2, log), keep(arrive/2, all),
                  col_type(thing/3, id, int), col_type(thing/3, payload, text),
                  col_type(thing/3, created_at_tick, int), keyed(thing/3, [1]) ],
                [ (thing(Id, Payload, Tick) <+ arrive(Id, Payload), now(Tick)) ]),
           [], [[ +arrive(1, 'a') ], [ +arrive(1, 'b') ]], Final, _),
       engine:rel_rows(thing/3, Final, Rows),
       Rows == [ thing(1, b, 2) ].          % birth tick 1 was overwritten

% T15. The CORRECT spelling is two ordinary edge rules -- pre/1 carries the
% birth tick forward, not/1 supplies the base case -- and it produces exactly
% the semantics H2 asks for: created_at pinned at 1, updated_at advancing to 3
% across three replaces. ZERO new constructs. So H2 is sugar at most.
t15 :- run(prog([ col_type(arrive/2, id, int), col_type(arrive/2, payload, text),
                  kind(arrive/2, log), keep(arrive/2, all),
                  col_type(thing/4, id, int), col_type(thing/4, payload, text),
                  col_type(thing/4, created_at_tick, int),
                  col_type(thing/4, updated_at_tick, int), keyed(thing/4, [1]) ],
                [ (thing(Id, Payload, Born, Tick) <+
                       arrive(Id, Payload), now(Tick),
                       pre(thing(Id, _Old, Born, _Was))),
                  (thing(Id, Payload, Tick, Tick) <+
                       arrive(Id, Payload), now(Tick),
                       not(thing(Id, _P, _B, _U))) ]),
           [], [[ +arrive(1, 'a') ], [ +arrive(1, 'b') ], [ +arrive(1, 'c') ]],
           Final, _),
       engine:rel_rows(thing/4, Final, Rows),
       Rows == [ thing(1, c, 1, 3) ].

go :-
    format("── time-plane lab, reference-engine receipts ───────────────────~n"),
    check('T1  set rel: identical re-arrival is ONE occurrence', t1),
    check('T2  log rel: identical re-arrival is TWO occurrences', t2),
    check('T3  set rel, rows differing by a column: TWO occurrences', t3),
    check('T4  keep(count) prunes and reports NO minus (the gap)', t4),
    check('T5  finalize over a pruning log fires nothing (silently)', t5),
    check('T6  retract_from_log throws (R7)', t6),
    check('T7  log_on_level_headed_rel throws', t7),
    check('T8  keep_on_non_log_rel throws', t8),
    check('T9  missing_retention throws', t9),
    check('T10 keyed_log_rel throws', t10),
    check('T11 a SET arrival consumes a log rel ordinal (global counter)', t11),
    check('T12 the arrival counter is shared BETWEEN log rels', t12),
    check('T13 same wire rows, one row on set plane, two on log plane', t13),
    check('T14 naive one-rule created_at is really updated_at (the trap)', t14),
    check('T15 created_at + updated_at expressible TODAY, zero new constructs', t15),
    failures(Failures),
    format("── ~w failure(s) ──────────────────────────────────────────────~n",
           [Failures]),
    ( Failures =:= 0 -> true ; halt(1) ).
