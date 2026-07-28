% scenarios.pl : Q4 CONTRADICTION HUNT + the two prospective fixtures.
%
% Every scenario whose desugared form is a plain kernel program is run on the
% REAL ORACLE (engine:run_program/5, read-only, exactly the way ticklog.pl
% consumes it). Only the two things the oracle has no machinery for at all
% (a primitive Ta queue; spilling instead of throwing at the drain cap) run
% on mf_model.
%
% Every expected log below was hand-computed from engine.pl BEFORE it was
% run. All twelve matched on the first execution; where a scenario surprised
% the author it says so in its own comment. The value of the scenario is the
% CONTRADICTION it exhibits, not the fact that the engine is self-consistent.

:- module(mf_scenarios, [ scenario/2, prospective_fixture/2 ]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module('../../conformance/engine').
:- use_module(model).
:- use_module(desugar).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

:- discontiguous scenario/2.
:- discontiguous prospective_fixture/2.

% scenario(Name, Goal) : Goal must succeed. Each Goal both RUNS the scenario
% and asserts its hand-computed log.

oracle_log(Prog, Initial, Schedule, DeltaTicks) :-
    run_program(Prog, Initial, Schedule, _Final, DeltaTicks).

oracle_throws(Prog, Initial, Schedule, Expected) :-
    catch(( run_program(Prog, Initial, Schedule, _, _), fail ), Thrown, true),
    Thrown == Expected.

% ═══ (a) transition rule under multi-key + multi-replace in one tick ════════
% CONTRADICTION 1 (highest severity). The flagship rule
%   changed(Key, Old, New) <+ finalize(cache(Key, Old)), next(cache(Key, New))
% cannot see intra-tick replaces. Two from_poll rows for ONE key in ONE tick
% produce ONE changed row reporting v0 -> v2: the intermediate v1 is gone.
% The cause is not the arm design, it is engine.pl:299-304 (carry-out is
% boundary-observable writes only, R2 rider) plus the set-diff boundary, and
% it is therefore not fixable inside the arm sugar.
%
% The multiplicity is a function of TICK BATCHING, not of the data: the same
% two polls delivered one per tick produce TWO changed rows. So the same
% program over the same inputs reports a different number of transitions
% depending on how the scheduler batched them.
%
% MULTIPLICITY TABLE (all four rows are graded below):
%   N replaces of ONE key in ONE tick, rel non-empty at tick start  -> 1 firing
%   N replaces of ONE key across N ticks                            -> N firings
%   1 replace each of M keys in ONE tick                            -> M firings
%   N replaces of ONE key in ONE tick, rel EMPTY at tick start      -> 0 firings

transition_prog(prog([ kind(from_poll/2, log), keep(from_poll/2, all),
                       keyed(latest/2, [1]),
                       kind(changed/3, log), keep(changed/3, all) ],
                     [ (latest(Key, Value) <+ from_poll(Key, Value)),
                       (changed(Key, Old, New) <+ departed(latest(Key, Old)),
                                                  latest(Key, New)) ])).

scenario(a1_two_replaces_one_tick_collapse_to_one_firing, Goal) :-
    transition_prog(Prog),
    Goal = ( oracle_log(Prog, [latest(cli, v0)],
                        [[+from_poll(cli, v1), +from_poll(cli, v2)]], Log),
             Log == [ [ -latest(cli, v0), +latest(cli, v2),
                        +from_poll(cli, v1), +from_poll(cli, v2) ],
                      [ +changed(cli, v0, v2) ],
                      [] ] ).

scenario(a2_two_replaces_two_ticks_give_two_firings, Goal) :-
    transition_prog(Prog),
    Goal = ( oracle_log(Prog, [latest(cli, v0)],
                        [[+from_poll(cli, v1)], [+from_poll(cli, v2)]], Log),
             Log == [ [ -latest(cli, v0), +latest(cli, v1), +from_poll(cli, v1) ],
                      [ -latest(cli, v1), +latest(cli, v2), +from_poll(cli, v2),
                        +changed(cli, v0, v1) ],
                      [ +changed(cli, v1, v2) ],
                      [] ] ).

scenario(a3_multi_key_one_replace_each_fires_per_key, Goal) :-
    transition_prog(Prog),
    Goal = ( oracle_log(Prog, [latest(cli, v0), latest(api, w0)],
                        [[+from_poll(cli, v1), +from_poll(api, w1)]], Log),
             Log == [ [ -latest(api, w0), -latest(cli, v0),
                        +latest(api, w1), +latest(cli, v1),
                        +from_poll(cli, v1), +from_poll(api, w1) ],
                      [ +changed(api, w0, w1), +changed(cli, v0, v1) ],
                      [] ] ).

scenario(a4_cold_start_gives_zero_firings, Goal) :-
    transition_prog(Prog),
    Goal = ( oracle_log(Prog, [],
                        [[+from_poll(cli, v1), +from_poll(cli, v2)]], Log),
             Log == [ [ +latest(cli, v2), +from_poll(cli, v1), +from_poll(cli, v2) ],
                      [] ] ).

% ═══ (b) finalize cascade cycle: quiescence vs drain cap ════════════════════
% A keyed rel whose departure writes a strictly larger value for its own key
% never reaches an equal-row no-op, so it never quiesces. engine.pl:373-379
% throws drain_overflow(100) LOUDLY. Held up as the correct behavior; the
% spill alternative is graded in the model below and is strictly worse.

scenario(b1_finalize_cycle_hits_the_drain_cap_loudly, Goal) :-
    Goal = oracle_throws(
        prog([ kind(start/1, log), keep(start/1, all), keyed(token/2, [1]) ],
             [ (token(1, One) <+ start(_), One := 1),
               (token(1, Next) <+ departed(token(1, Value)), Next := Value + 2) ]),
        [token(1, 0)], [[+start(go)]], drain_overflow(100)).

% Same shape in the model, so the model is shown to agree with the oracle
% before it is trusted for the spill question.
scenario(b2_model_agrees_the_cycle_is_loud, Goal) :-
    Goal = ( catch(( mrun(mprog([], [ mrule(arr(token(Value)), [calc(Next := Value + 1)],
                                            token(Next), ti) ]),
                          [[+token(0)]], ti_only, _, _), fail ),
                   Thrown, true),
             Thrown == drain_overflow(100) ).

% SLOT-SPILL, graded: spilling the over-cap carry into a Ta queue instead of
% throwing does NOT fix the nontermination. It converts a loud failure into a
% silent one: the run returns a log, and the work sits in a queue nothing
% will ever deliver (the schedule is exhausted). The residue is the receipt.
scenario(b3_spill_at_cap_is_silent_loss_not_a_fix, Goal) :-
    Goal = ( mrun(mprog([], [ mrule(arr(token(Value)), [calc(Next := Value + 1)],
                                    token(Next), ti) ]),
                  [[+token(0)]], spill_at_cap, Log, Residue),
             Residue \== [],
             length(Log, LogLength), LogLength =:= 101 ).

% ═══ (c) self-retraction: finalize feeding an edge that retracts more ═══════
% Bounded and loud enough: one departure per drain tick, terminating with the
% chain. The design SURVIVES this one; it is here because it is the scenario
% most likely to be assumed pathological, and it is not.

scenario(c1_self_retraction_chain_is_bounded, Goal) :-
    Goal = ( oracle_log(
                prog([ kind(kill/1, log), keep(kill/1, all),
                       keyed(slot/2, [1]), kind(chain/2, set) ],
                     [ (slot(1, off) <+ kill(_)),
                       (live(Id) <- slot(Id, on)),
                       (slot(Next, off) <+ departed(live(Id)), chain(Id, Next)) ]),
                [slot(1, on), slot(2, on), slot(3, on), chain(1, 2), chain(2, 3)],
                [[+kill(go)]], Log),
             Log == [ [ -live(1), -slot(1, on), +slot(1, off), +kill(go) ],
                      [ -live(2), -slot(2, on), +slot(2, off) ],
                      [ -live(3), -slot(3, on), +slot(3, off) ],
                      [] ] ).

% ═══ (d) not() in an arm: stratification preserved? ═════════════════════════
% In a `->` arm (level rule): PRESERVED. level_eval.pl:121-142 stratifies and
% throws not_stratified on a cycle; arms change nothing, since the desugared
% rule set is what gets stratified.
%
% In a `+>` arm (edge rule): NOT STRATIFIED AT ALL, and this is
% CONTRADICTION 2. engine.pl:284-286 hands level_closure/5 only PlainLevel
% and AggRules; edge rules are never stratified. So not(EdgeHeadedRel) in an
% edge body reads the store MID-OCCURRENCE-LOOP, and the answer depends on
% the order two rows happened to arrive inside one tick. Two runs of the same
% program with the same two rows in a different order produce DIFFERENT
% output, silently, with no diagnostic anywhere.

order_dependent_prog(prog([ kind(src/1, log), keep(src/1, all),
                            keyed(mark/1, [1]),
                            kind(out/1, log), keep(out/1, all) ],
                          [ (mark(Item) <+ src(Item)),
                            (out(Item) <+ src(Item), not(mark(_))) ])).

scenario(d1_edge_negation_is_arrival_order_dependent, Goal) :-
    order_dependent_prog(Prog),
    Goal = ( oracle_log(Prog, [], [[+src(a), +src(b)]], LogAB),
             oracle_log(Prog, [], [[+src(b), +src(a)]], LogBA),
             LogAB == [ [ +mark(a), +mark(b), +src(a), +src(b), +out(a) ], [] ],
             LogBA == [ [ +mark(a), +mark(b), +src(b), +src(a), +out(b) ], [] ],
             LogAB \== LogBA ).

% The level-arm half of (d): stratification is preserved and the guard is
% NOT order dependent, because MidLevel is frozen for the whole loop.
scenario(d2_level_negation_is_order_independent, Goal) :-
    Prog = prog([ kind(src/1, log), keep(src/1, all), kind(out/1, log), keep(out/1, all) ],
                [ (marked(Item) <- src(Item), Item == a),
                  (out(Item) <+ src(Item), not(marked(Item))) ]),
    Goal = ( oracle_log(Prog, [], [[+src(a), +src(b)]], LogAB),
             oracle_log(Prog, [], [[+src(b), +src(a)]], LogBA),
             maplist(sort, LogAB, SortedAB), maplist(sort, LogBA, SortedBA),
             SortedAB == SortedBA ).

% ═══ (e) two-axis nesting: is nesting order forced? ═════════════════════════
% NO, and the check is that both spellings desugar to the SAME rule set.
% Lifecycle-outer:  next(resp(K,'Fresh')) +> ... / finalize(resp(K,_)) +> ...
% Enum-outer:       'Fresh' => { next +> ... } etc.
% Both are just (Trigger, Guards, Head) triples, so the desugar target is
% identical up to arm order. The REAL finding is elsewhere: see
% e2_match_over_promises_exhaustiveness.

scenario(e1_nesting_order_is_not_forced, Goal) :-
    LifecycleOuter = match(resp(key_column, status_column),
                           [ arm('+>', next(resp(key_column, 'Fresh')), [], fresh_hit(key_column)),
                             arm('+>', next(resp(key_column, 'Error')), [], error_hit(key_column)) ]),
    EnumOuter      = match(resp(key_column, status_column),
                           [ arm('+>', next(resp(key_column, 'Fresh')), [], fresh_hit(key_column)),
                             arm('+>', next(resp(key_column, 'Error')), [], error_hit(key_column)) ]),
    Goal = ( desugar_match(LifecycleOuter, RulesA),
             desugar_match(EnumOuter, RulesB),
             RulesA == RulesB,
             RulesA == [ (fresh_hit(key_column) <+ only(resp(key_column, 'Fresh'))),
                         (error_hit(key_column) <+ only(resp(key_column, 'Error'))) ] ).

% CONTRADICTION 5. The word `match` (Rust, and every ML descendant) promises
% exhaustiveness checking. Under the typed-columns ruling of 2026-07-28 the
% only column types are int and text; there are no enum types, so "did the
% arms cover every tag" is not decidable for ANY nesting. The construct
% borrows a word whose whole value is a check the language cannot perform.
% Graded by showing a match block with a tag the arms do not cover desugars
% CLEANLY, with no complaint of any kind.
scenario(e2_match_over_promises_exhaustiveness, Goal) :-
    Goal = ( desugar_match(match(resp(key_column, status_column),
                                 [ arm('+>', next(resp(key_column, 'Fresh')), [], hit(key_column)) ]),
                           Rules),
             Rules = [_],
             % nothing anywhere knows that 'Error' exists and is uncovered
             \+ ( member(Rule, Rules), sub_term('Error', Rule) ) ).

% ═══ (f) Ta indistinguishability + THE DISSOLUTION HYPOTHESIS ═══════════════
% User hypothesis (2026-07-28): Ta dissolves entirely, the way
% clock_residency dissolved the cadence construct. An async carry is an
% ordinary edge append into a pending rel plus an ordinary rule consuming it
% on a later tick.
%
% GRADED BOTH WAYS in the model, on the same schedule:
%   f1  primitive Ta is NONDETERMINISTIC: the delivery tick is an engine
%       choice, so the same program on the same schedule under two engine
%       choices yields two different tick logs. That breaks the standing
%       item-9 grading law (logs diffed byte-for-byte against the oracle).
%   f2  the pending-rel encoding REPRODUCES primitive Ta exactly: strip the
%       pending rel's own deltas and the two logs are identical up to one
%       trailing quiescence tick, which is the ONE named difference.
%   f3  the encoding's extra tick is not noise, it is the edge write's own
%       carry (engine.pl:302-304). Named, not hidden.
%
% The encoding also wins on three things primitive Ta cannot offer, none of
% which need a check because they are structural: the queue is a durable rel
% (the endurance law already covers it), the queue is VISIBLE in the tick log
% (self-diagnosis law), and the queued rows are ordinary rows, therefore
% MATCHABLE with ordinary arms. That is the answer to "is the carry itself
% matchable": under dissolution, yes, for free.

ta_primitive_prog(mprog([], [ mrule(arr(src(Item)), [], out(Item), ta) ])).
ta_pending_prog(mprog([], [ mrule(arr(src(Item)), [], pending(Item), ti),
                            mrule(arr(pending(Item)), [], out(Item), ti) ])).
ta_schedule([[+src(a)], []]).

scenario(f1_primitive_ta_log_depends_on_an_engine_choice, Goal) :-
    ta_primitive_prog(Prog), ta_schedule(Schedule),
    Goal = ( mrun(Prog, Schedule, ta_after(1), LogOne, []),
             mrun(Prog, Schedule, ta_after(2), LogTwo, []),
             LogOne \== LogTwo,
             LogOne == [ line(1, [+src(a)]), line(2, [+out(a)]) ],
             LogTwo == [ line(1, [+src(a)]), line(2, []), line(3, [+out(a)]) ] ).

scenario(f2_pending_rel_encoding_reproduces_primitive_ta, Goal) :-
    ta_primitive_prog(PrimitiveProg), ta_pending_prog(PendingProg), ta_schedule(Schedule),
    Goal = ( mrun(PrimitiveProg, Schedule, ta_after(1), PrimitiveLog, []),
             mrun(PendingProg, Schedule, ti_only, PendingLog, []),
             mlog_strip(PendingLog, pending/1, Stripped),
             trim_trailing_empty(Stripped, Trimmed),
             Trimmed == PrimitiveLog ).

scenario(f3_the_only_difference_is_one_quiescence_tick, Goal) :-
    ta_primitive_prog(PrimitiveProg), ta_pending_prog(PendingProg), ta_schedule(Schedule),
    Goal = ( mrun(PrimitiveProg, Schedule, ta_after(1), PrimitiveLog, []),
             mrun(PendingProg, Schedule, ti_only, PendingLog, []),
             length(PrimitiveLog, PrimitiveTicks),
             length(PendingLog, PendingTicks),
             PendingTicks =:= PrimitiveTicks + 1,
             last(PendingLog, line(_, [])) ).

% The encoding has NO engine knob at all: ti_only is its only legal policy,
% so its log is a function of program plus schedule, which is exactly what
% the item-9 grading law requires.
scenario(f4_pending_encoding_has_no_engine_knob, Goal) :-
    ta_pending_prog(Prog), ta_schedule(Schedule),
    Goal = ( mrun(Prog, Schedule, ti_only, LogOne, []),
             mrun(Prog, Schedule, ti_only, LogTwo, []),
             LogOne == LogTwo,
             mlog_refs(LogOne, Refs),
             memberchk(pending/1, Refs) ).

trim_trailing_empty(Log, Trimmed) :-
    reverse(Log, Reversed),
    drop_empty_prefix(Reversed, TrimmedReversed),
    reverse(TrimmedReversed, Trimmed).

drop_empty_prefix([line(_, []) | Rest], Out) :- !, drop_empty_prefix(Rest, Out).
drop_empty_prefix(Log, Log).

% ═══ (g) one-body-one-time-cut ══════════════════════════════════════════════
% A body holding BOTH a finalize atom and a next atom has a coherent reading,
% and it is FORCED, not chosen: engine.pl:162-166 substitutes away only the
% ONE departed goal the occurrence matched, and body.pl:102 makes departed/1
% unsatisfiable as a read. So the departure is always the cut and the next()
% atom always degrades to a store read.
%
% CONTRADICTION 3 falls straight out of that: a body with TWO finalize atoms
% is STATICALLY DEAD. Whichever departure fires, the other departed goal
% remains in the body and fails. Nothing refuses it, nothing warns, and the
% rule simply never produces a row.

scenario(g1_the_cut_is_the_departure_and_next_degrades_to_a_read, Goal) :-
    Goal = ( oracle_log(
                prog([ keyed(x/1, [1]), kind(seen/1, set),
                       kind(both/2, log), keep(both/2, all) ],
                     [ (both(Left, Right) <+ departed(x(Left)), seen(Right)) ]),
                [x(1)], [[-x(1), +seen(9)]], Log),
             Log == [ [ -x(1), +seen(9) ], [ +both(1, 9) ], [] ] ).

scenario(g2_two_finalize_atoms_is_a_silently_dead_rule, Goal) :-
    Goal = ( oracle_log(
                prog([ keyed(x/1, [1]), keyed(y/1, [1]),
                       kind(both/2, log), keep(both/2, all) ],
                     [ (both(Left, Right) <+ departed(x(Left)), departed(y(Right))) ]),
                [x(1), y(2)], [[-x(1), -y(2)]], Log),
             Log == [ [ -x(1), -y(2) ], [] ],
             \+ ( member(Deltas, Log), member(Delta, Deltas),
                  ( Delta = +Row ; Delta = -Row ), functor(Row, both, 2) ) ).

% ═══ (h) finalize binding a row no longer in the table ══════════════════════
% The flagship join WORKS: the departed goal binds Old from the dep(Row)
% payload and is substituted away before solving, so the same rel can be read
% again in the same body and yields the NEW row. h1 is that receipt.
%
% h2 is the hazard nobody stated: on a pure DELETE (no replacement) the join
% has nothing to bind and the rule silently produces nothing. A program that
% wants delete telemetry needs a SECOND arm, and no diagnostic says so.

scenario(h1_old_and_new_of_one_rel_coexist_in_one_body, Goal) :-
    transition_prog(Prog),
    Goal = ( oracle_log(Prog, [latest(cli, v0)], [[+from_poll(cli, v1)]], Log),
             Log == [ [ -latest(cli, v0), +latest(cli, v1), +from_poll(cli, v1) ],
                      [ +changed(cli, v0, v1) ],
                      [] ] ).

scenario(h2_pure_delete_silently_produces_nothing, Goal) :-
    Goal = ( oracle_log(
                prog([ keyed(latest/2, [1]), kind(changed/3, log), keep(changed/3, all) ],
                     [ (changed(Key, Old, New) <+ departed(latest(Key, Old)), latest(Key, New)) ]),
                [latest(cli, v0)], [[-latest(cli, v0)]], Log),
             Log == [ [ -latest(cli, v0) ], [] ] ).

% ═══ CONTRADICTION 4: finalize on a Log rel is statically dead ══════════════
% Log rels never emit a -delta: boundary_deltas/6 filters set removals with
% delta_ref_is_set/3 (engine.pl:331-335) and the Log leg (:328) emits +Row
% only. DepartureCarry is built from -Row deltas (:307-311). So a finalize
% arm over a Log rel can never fire, however the rel is retained.
%
% Worse, the case where a row genuinely LEAVES a Log rel is retention:
% apply_retention/3 (:262-275) runs at :293, BEFORE boundary_deltas/6 at
% :298, and prunes rows out of the store with no delta of any kind. The
% pruned row leaves no trace in the tick log at all.

scenario(x1_finalize_on_a_log_rel_never_fires, Goal) :-
    Goal = ( oracle_log(
                prog([ kind(ev/1, log), keep(ev/1, count(1)),
                       kind(gone/1, log), keep(gone/1, all) ],
                     [ (gone(Item) <+ departed(ev(Item))) ]),
                [], [[+ev(a)], [+ev(b)]], Log),
             Log == [ [ +ev(a) ], [ +ev(b) ] ],
             \+ ( member(Deltas, Log), member(Delta, Deltas),
                  ( Delta = +Row ; Delta = -Row ), functor(Row, gone, 1) ) ).

% The pruned row is invisible in the log AND absent from the final state:
% both halves of the receipt, so nobody can claim the delta merely moved.
scenario(x2_retention_prune_leaves_no_delta_at_all, Goal) :-
    Goal = ( run_program(
                prog([ kind(ev/1, log), keep(ev/1, count(1)) ], []),
                [], [[+ev(a)], [+ev(b)]], Final, Log),
             Final == [ev(b)],
             Log == [ [ +ev(a) ], [ +ev(b) ] ] ).

% ═══ prospective fixture 1: departure rename ════════════════════════════════
% engine_core.pl:117 departed_fires_next_tick_on_retraction, re-expressed as
% a match block with a finalize arm. Graded on the TICK LOG, which is the
% grading currency (item 9), not on term equality: the arm form emits the
% marked spelling only(departed(...)) where the corpus fixture writes it
% unmarked. Both were run; the logs are identical.

prospective_fixture(departure_rename_as_finalize_arm, Goal) :-
    MatchBlock = match(mirror(item_column),
                       [ arm('+>', finalize(mirror(item_column)), [now(tick_column)],
                             closed_at(item_column, tick_column)) ]),
    Goal = ( desugar_match(MatchBlock, [ArmRule]),
             ArmRule == (closed_at(item_column, tick_column) <+
                           only(departed(mirror(item_column))), now(tick_column)),
             % the corpus spelling, unmarked
             CorpusRule = (closed_at(Item, Tick) <+ departed(mirror(Item)), now(Tick)),
             ArmProg = prog([ kind(source_row/1, set),
                              kind(closed_at/2, log), keep(closed_at/2, all) ],
                            [ (mirror(Row) <- source_row(Row)),
                              (closed_at(Thing, When) <+
                                 only(departed(mirror(Thing))), now(When)) ]),
             CorpusProg = prog([ kind(source_row/1, set),
                                 kind(closed_at/2, log), keep(closed_at/2, all) ],
                               [ (mirror(Row2) <- source_row(Row2)), CorpusRule ]),
             Schedule = [[+source_row(alpha)], [-source_row(alpha)]],
             run_program(ArmProg, [], Schedule, ArmFinal, ArmLog),
             run_program(CorpusProg, [], Schedule, CorpusFinal, CorpusLog),
             ArmLog == CorpusLog,
             ArmFinal == CorpusFinal,
             ArmFinal == [closed_at(alpha, 3)],
             length(ArmLog, 4) ).

% ═══ prospective fixture 2: the transition rule ═════════════════════════════
% The flagship, with the hand-written log the design implies AND the log the
% engine actually produces. They are the same only because the schedule
% delivers one poll per tick; a1 above is the same fixture batched
% differently and it loses a transition.

prospective_fixture(transition_rule_keyed_replace_drives_changed, Goal) :-
    transition_prog(Prog),
    Goal = ( run_program(Prog, [latest(cli, v0)],
                         [[+from_poll(cli, v1)], [+from_poll(cli, v2)]], Final, Log),
             Log == [ [ -latest(cli, v0), +latest(cli, v1), +from_poll(cli, v1) ],
                      [ -latest(cli, v1), +latest(cli, v2), +from_poll(cli, v2),
                        +changed(cli, v0, v1) ],
                      [ +changed(cli, v1, v2) ],
                      [] ],
             memberchk(changed(cli, v0, v1), Final),
             memberchk(changed(cli, v1, v2), Final) ).
