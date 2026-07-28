% desugar.pl : THREAD 5 (the optional one), the level-rule-as-signed-edge
% claim.
%
% Claim under test: a level rule is sugar for an edge rule over SIGNED
% deltas, so `out(x) <- src(x)` could be executed as a plus arm plus a minus
% arm and produce an identical tick log.
%
% Verdict: the plus half is exact and the minus half is inexpressible. The
% kernel has no retracting edge head, and even if it had one the departure
% carry would deliver it one tick late.

:- module(ca_desugar, [ desugar_scenario/2, desugar_half/3 ]).

:- use_module(library(lists)).
:- use_module(oracle).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

:- discontiguous desugar_scenario/2.

% desugar_half(Half, Verdict, Why)
desugar_half(plus, identical,
             'the level row and the edge write both land in the arrival tick, so the two logs agree byte for byte on the plus side').
desugar_half(minus, inexpressible,
             'edge heads only append or replace; nothing in the kernel retracts, so the minus half has no edge form at all. A finalize arm can observe the departure but only writes a NEW row, and it fires one drain tick after the minus delta').

%
% .dl surface, level:
%
%   rel src(item: text)
%   out(item) <- src(item).
%
% .dl surface, signed edge (the claim):
%
%   rel src(item: text)
%   rel out(item: text) key(item)
%   out(item) <+ src(item).                    % the plus arm
%   ??? <+ finalize(src(item)).                % the minus arm has no head form
%
% rx lowering, level:
%   src$.pipe(scan(applySignedDelta, new Set()), map(live => derive(live)),
%             distinctUntilChanged(setEquals))
%   -- the whole set is the value, so a removal simply produces a smaller set.
%
% rx lowering, signed edge:
%   merge(srcAdds$.pipe(map(row => ({ sign: 1, row }))),
%         srcRemoves$.pipe(map(row => ({ sign: -1, row }))))
%     .pipe(scan(applySignedDelta, new Set()))
%   -- this composes in rx precisely BECAUSE the scan owns a set it can
%   shrink. The kernel's edge writes have no shrinking counterpart, which is
%   the whole gap: rx can express the desugar and the kernel cannot.

level_form(prog([ kind(src/1, set) ], [ (out(Item) <- src(Item)) ])).

edge_form(prog([ kind(src/1, set), keyed(out/1, [1]) ], [ (out(Item) <+ src(Item)) ])).

edge_form_with_finalize(
    prog([ kind(src/1, set), keyed(out/1, [1]), keyed(gone/1, [1]) ],
         [ (out(Item) <+ src(Item)),
           (gone(Item) <+ finalize(src(Item))) ])).

% ═══ ROUND 1 : the plus half is exact ══════════════════════════════════════

% ROUND 1 asserted "byte for byte". ROUND 4 broke that on the tick count and
% left this: every DELTA agrees, in the same tick, in the same order. The
% residual difference is the trailing quiescence tick, graded in rounds.pl.
desugar_scenario(r1_the_plus_half_lands_the_same_deltas_in_the_arrival_tick, Goal) :-
    level_form(Level), edge_form(Edge),
    Goal = ( oracle_log(Level, [], [[ +src(a) ]], [LevelFirst | LevelRest]),
             oracle_log(Edge, [], [[ +src(a) ]], [EdgeFirst | EdgeRest]),
             LevelFirst == [ +out(a), +src(a) ],
             LevelFirst == EdgeFirst,
             LevelRest == [], EdgeRest == [[]] ).

% ═══ ROUND 2 : the minus half is not a shift, it is a gap ══════════════════
% A first reading is "the edge form is one tick late". It is not late; it
% never happens. The level form retracts out(a) at the same tick src(a)
% leaves; the edge form leaves out(a) in the store forever.

desugar_scenario(r2_the_level_form_retracts_in_the_departure_tick, Goal) :-
    level_form(Level),
    Goal = ( oracle_log_final(Level, [], [[ +src(a) ], [ -src(a) ]], Final, Log),
             Log == [ [ +out(a), +src(a) ], [ -out(a), -src(a) ] ],
             Final == [] ).

desugar_scenario(r2_the_edge_form_never_retracts_at_all, Goal) :-
    edge_form(Edge),
    Goal = ( oracle_log_final(Edge, [], [[ +src(a) ], [ -src(a) ]], Final, Log),
             Log == [ [ +out(a), +src(a) ], [ -src(a) ] ],
             final_has(Final, out(a)) ).

% Adding the finalize arm does not close it: the arm fires one drain tick
% after the minus delta and can only WRITE a new row. out(a) survives.
desugar_scenario(r2_a_finalize_arm_writes_a_row_and_cannot_remove_one, Goal) :-
    edge_form_with_finalize(Edge),
    Goal = ( oracle_log_final(Edge, [], [[ +src(a) ], [ -src(a) ]], Final, Log),
             Log == [ [ +out(a), +src(a) ], [ -src(a) ], [ +gone(a) ], [] ],
             final_has(Final, out(a)), final_has(Final, gone(a)) ).

% ═══ ROUND 3 : the bound on the claim, stated ══════════════════════════════

desugar_scenario(r3_the_claim_holds_on_the_plus_half_and_fails_on_the_minus_half, Goal) :-
    Goal = ( desugar_half(plus, identical, _),
             desugar_half(minus, inexpressible, _),
             findall(Half, desugar_half(Half, _, _), Halves),
             msort(Halves, Sorted), Sorted == [minus, plus] ).
