% invariants.pl : Q6 INVARIANT PRESERVATION.
%
% invariant(Name, Verdict, Note) with Verdict:
%   preserved(ProofSketch)  a check below backs it where a check is possible
%   broken(ScenarioName)    a counterexample scenario names the break
%   needs_rule(Slot)        survives only under a rule nobody has stated
%
% invariant_check(Name, Goal) supplies the executable half where one exists.

:- module(mf_invariants, [ invariant/3, invariant_check/2 ]).

:- use_module(library(lists)).
:- use_module('../../conformance/engine').
:- use_module(desugar).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).

:- discontiguous invariant/3.
:- discontiguous invariant_check/2.

% ═══ 1. sugar grounds out ═══════════════════════════════════════════════════

invariant(sugar_grounds_out_lifecycle_arms, preserved(
  'next / finalize / bare-atom arms under +> each emit exactly one kernel rule with an only/1 trigger; ARCH.pl sugar_grounds_out holds. Checked by desugaring a two-arm block and comparing to the hand-written kernel pair.'), ok).
invariant(sugar_grounds_out_complete_arm, needs_rule('SLOT-COMPLETE'),
  'complete has no kernel form. This lab found a candidate that needs no construct: a scope closing IS a retraction of the scope row, so complete == finalize(scope_row), and rx groupBy s duration selector is the exact lowering (lowering.pl complete_arm, graded direct). Recorded, not resolved.').
invariant(sugar_grounds_out_async_marker, needs_rule('SLOT-TA-MARK'),
  'no kernel form exists and none can: engine.pl run_ticks/7 :367-379 has two legs, scheduled and drain, and no third queue. Under the dissolution hypothesis the sugar grounds out to a pending rel plus a consuming rule, which are both already kernel (scenarios f2-f4).').

% ═══ 2. one rel = one rule kind ═════════════════════════════════════════════
% The source-major shape makes the violation the NATURAL thing to write: arms
% in one block share a subject, and heading one rel from a +> arm and a -> arm
% in the same block is a two-line edit. Nothing in the sugar refuses it, and
% the standing law (CLAUDE.md, and the shipped engine bail) is what breaks.

invariant(one_rel_one_rule_kind, needs_rule('SLOT-LEVEL-ARMS'),
  'a block with a +> arm and a -> arm heading the SAME rel desugars cleanly into (Head <+ _) and (Head <- _). Restricting -> arms to guards and patterns (the SLOT-LEVEL-ARMS question, which this lab REFUTES as already-restricted and instead recommends restricting by HEAD) is what closes it.').

invariant_check(one_rel_one_rule_kind, Goal) :-
    Goal = ( desugar_match(match(source_row(item_column),
                                 [ arm('+>', next(source_row(item_column)), [], mirror(item_column)),
                                   arm('->', source_row(item_column), [], mirror(item_column)) ]),
                           Rules),
             Rules = [EdgeRule, LevelRule],
             EdgeRule = (mirror(_) <+ _),
             LevelRule = (mirror(_) <- _) ).

% ═══ 3. stratification ══════════════════════════════════════════════════════

invariant(stratification_in_level_arms, preserved(
  'level_eval.pl:121-142 relaxes strata over the DESUGARED rule set and throws not_stratified on a cycle; arms change nothing about which rules exist. Scenario d2 shows a level guard is order-independent.'), ok).
invariant(stratification_in_event_arms, broken(d1_edge_negation_is_arrival_order_dependent),
  'engine.pl:284-286 hands level_closure/5 only PlainLevel and AggRules. Edge rules are NEVER stratified, so not(EdgeHeadedRel) in a +> arm reads the store mid-occurrence-loop and the answer depends on arrival order inside one tick. Two runs, same rows, different order, different output, no diagnostic. Predates the arm design; the arms make it easier to write.').

% ═══ 4. occurrence multiplicity ═════════════════════════════════════════════

invariant(occurrence_multiplicity, preserved(
  'one firing per occurrence holds exactly. The consequence nobody stated: carry-out is boundary-observable writes only (engine.pl:299-304), so N intra-tick replaces of one key yield ONE departure occurrence. Scenario a1 vs a2: the same two polls give 1 or 2 transitions depending only on how the scheduler batched them.'), ok).

% ═══ 5. R7 boundary diff ════════════════════════════════════════════════════

invariant(r7_boundary_diff, preserved(
  'finalize arms are DEFINED on the minus side of R7 (engine.pl:331-341); they add no new delta kind and no new phase. Checked: the number of finalize firings equals the number of minus deltas of the subject rel.'), ok).

invariant_check(r7_boundary_diff, Goal) :-
    Goal = ( run_program(
                prog([ kind(source_row/1, set),
                       kind(closed_at/2, log), keep(closed_at/2, all) ],
                     [ (mirror(Item) <- source_row(Item)),
                       (closed_at(Thing, When) <+ only(departed(mirror(Thing))), now(When)) ]),
                [], [[+source_row(alpha), +source_row(beta)], [-source_row(alpha), -source_row(beta)]],
                Final, Log),
             findall(Row, ( member(Deltas, Log), member(-Row, Deltas), functor(Row, mirror, 1) ),
                     Departures),
             length(Departures, DepartureCount),
             findall(Row2, ( member(Row2, Final), functor(Row2, closed_at, 2) ), Firings),
             length(Firings, FiringCount),
             DepartureCount =:= FiringCount,
             DepartureCount =:= 2 ).

% ═══ 6. retention / keep ════════════════════════════════════════════════════

invariant(retention_keep, broken(x1_finalize_on_a_log_rel_never_fires),
  'a Log rel never emits a minus delta (engine.pl:328 emits plus only; :331-335 filters set removals by delta_ref_is_set/3), so a finalize arm over a Log rel is STATICALLY DEAD and nothing refuses it. And the one case where a Log row genuinely leaves, retention, prunes at :293 BEFORE boundary_deltas/6 at :298 and emits nothing at all (scenario x2). A static refusal finalize_on_log_rel is the missing rule.').

% ═══ 7. content-addressed effect identity + support refcount ════════════════

invariant(content_addressed_effect_identity, preserved(
  'a finalize arm over an effect-result rel fires on the cache row s deletion, which under the salt ruling is store semantics and deterministic. The reading that must be stated: finalize means THE ROW LEFT THE STORE, never THE WORLD WORK STOPPED. The effect_abort ruling is explicit that cancellation is best-effort and never semantic ("no arrow stop exist, is lie"), so a finalize arm used as a compensation hook is depending on something the ruling denies.'), ok).

% ═══ 8. exactly-once endurance ══════════════════════════════════════════════
% What replays after a crash between drain ticks? Nothing, because the carry
% set is not stored anywhere. engine.pl threads CarryOut as a runtime TERM
% through run_ticks/7 (:370-379). tsv2 does not even keep the rows: it
% reduces carry to a boolean (tickLoop.ts:31, emit_ts.pl:560-561) and
% re-derives triggers from this tick s own arrivals, which is why
% analyze.pl s check_edge_body_refs_not_derived refuses edge_trigger_is_derived
% outright. So a lifecycle arm whose firing is pending in the carry set at
% crash time never fires and nothing replays it.

invariant(exactly_once_endurance, broken(carry_is_not_a_stored_rel),
  'the Ti carry set is engine state in BOTH implementations, never a rel. A finalize arm whose departure occurrence is sitting in CarryOut when the process dies loses that firing with no trace. The pending-rel encoding from the dissolution hypothesis fixes this for free, because a pending row is an ordinary durable row the endurance law already covers. This is an independent argument for dissolution that has nothing to do with Ta.').

% The structural receipt: a departure occurrence exists between tick 2 and
% tick 3 of the departure fixture, and NO rel anywhere in the tick log holds
% it. If it were a stored rel it would appear as a delta.
invariant_check(exactly_once_endurance, Goal) :-
    Goal = ( run_program(
                prog([ kind(source_row/1, set),
                       kind(closed_at/2, log), keep(closed_at/2, all) ],
                     [ (mirror(Item) <- source_row(Item)),
                       (closed_at(Thing, When) <+ only(departed(mirror(Thing))), now(When)) ]),
                [], [[+source_row(alpha)], [-source_row(alpha)]], _Final, Log),
             nth1(2, Log, TickTwo),
             memberchk(-mirror(alpha), TickTwo),
             nth1(3, Log, TickThree),
             memberchk(+closed_at(alpha, 3), TickThree),
             % between them the pending departure occurrence is invisible:
             % no rel in ANY tick of the log carries it.
             findall(Ref,
                     ( member(Deltas, Log), member(Delta, Deltas),
                       ( Delta = +Row ; Delta = -Row ),
                       functor(Row, Name, Arity), Ref = Name/Arity ),
                     Refs0),
             sort(Refs0, Refs),
             Refs == [closed_at/2, mirror/1, source_row/1] ).

% ═══ 9. sugar grounds out, executable half ══════════════════════════════════

invariant_check(sugar_grounds_out_lifecycle_arms, Goal) :-
    Goal = ( desugar_match(match(cache(key_column, value_column),
                                 [ arm('+>', next(cache(key_column, value_column)), [],
                                       added(key_column)),
                                   arm('+>', finalize(cache(key_column, value_column)), [],
                                       dropped(key_column)) ]),
                           Rules),
             Rules == [ (added(key_column) <+ only(cache(key_column, value_column))),
                        (dropped(key_column) <+ only(departed(cache(key_column, value_column)))) ] ).
