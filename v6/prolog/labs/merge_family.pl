% merge_family.pl : does the rxjs merge family lower to plain rules?
%
% Run:     swipl -q -l v6/prolog/labs/merge_family.pl -g go -g halt
% Trace:   swipl -q -l v6/prolog/labs/merge_family.pl -g report -g halt
%
% Claim under test (LANG.md 2026-07-27): merge / mergeByKey / mergeByKeyScan
% are not operators at all, they are shapes of ordinary rules.
%
%   merge           = several edge rules, one head.
%   mergeByKey      = the same, with a Key(Type) column on the head. The key
%                     makes last-write-per-key the semantics, not a construct.
%   mergeByKeyScan  = the same, plus `pre(head)` in the body. Redux
%                     combineReducers: event rels in, one keyed state rel out,
%                     each arm a rule.
%
% This file carries a reference interpreter for the tick, small on purpose:
% a tick takes (CurrentRows, ArrivingDeltas, Rules) and returns
% (NextRows, EmittedDeltas). No sqlite, no rx; the lowering argument lives in
% merge_family.md.

:- use_module('../src/grader.pl').
:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(library(ordsets)).

:- op(1150, xfx, <-).       % LEVEL rule: maintained view, consequences retract
:- op(1150, xfx, <+).       % EDGE rule:  fires on arrivals, appends

:- dynamic current_program/1.
:- discontiguous in_program/1, rule_of/2, decl_of/2, scenario/4, check/2.

% ═══ the reader hook ═══════════════════════════════════════════════════════
% `in_program(Name).` opens a program; every following bare `<-` / `<+` clause
% becomes a rule_of/2 fact for it. The lab body then reads as the surface
% language instead of as quoted terms. Same machinery as rel_island.pl.

term_expansion(in_program(Name), [in_program(Name)]) :-
    retractall(current_program(_)),
    assertz(current_program(Name)).

term_expansion((Head <+ Body), rule_of(Name, (Head <+ Body))) :-
    current_program(Name).
term_expansion((Head <- Body), rule_of(Name, (Head <- Body))) :-
    current_program(Name).

% The one syntax experiment: a `scan` cluster that states the head rel and the
% pre-read ONCE and lists the arms under it. Expands to the plain edge rules,
% no new arrow. Verdict on whether it earns its keep is in the .md.
term_expansion(scan(KeyHead, pre(Slot), Arms), Clauses) :-
    current_program(Name),
    findall(rule_of(Name, Rule),
            ( member(arm(Event, Expr), Arms),
              scan_arm(KeyHead, Slot, Event, Expr, Rule) ),
            Clauses).

scan_arm(KeyHead, Slot, Event, Expr, (NewHead <+ (Event2, pre(OldHead), Value is Expr2))) :-
    copy_term(cluster(KeyHead, Slot, Event, Expr),
              cluster(KeyHead2, Slot2, Event2, Expr2)),
    KeyHead2 =.. [Rel | KeyArgs],
    append(KeyArgs, [Slot2],  OldArgs), OldHead =.. [Rel | OldArgs],
    append(KeyArgs, [Value], NewArgs), NewHead =.. [Rel | NewArgs].

% ═══ the tick, as a reference interpreter over terms ═══════════════════════

rel_of(Atom, Name/Arity) :- functor(Atom, Name, Arity).

rule_kind_of(Program, Ref, level)  :- rule_of(Program, (Head <- _)), rel_of(Head, Ref), !.
rule_kind_of(Program, Ref, edge)   :- rule_of(Program, (Head <+ _)), rel_of(Head, Ref), !.
rule_kind_of(_,       _,   source).

level_rel(Program, Ref) :- rule_kind_of(Program, Ref, level).

decl_key(Program, Ref, KeyPositions) :- decl_of(Program, keyed(Ref, KeyPositions)).

key_of(KeyPositions, Row, Key) :-
    Row =.. [_ | Args],
    findall(Column, ( member(Position, KeyPositions), nth1(Position, Args, Column) ), Key).

% ── body solving: a body is ONE time cut. `pre` is the only escape, and it
%    reads the tick's STARTING rows, never the rows this tick is writing.
solve(true,           _,    _)    :- !.
solve((Left, Right),  Rows, Prev) :- !, solve(Left, Rows, Prev), solve(Right, Rows, Prev).
solve(not(Goal),      Rows, Prev) :- !, \+ solve(Goal, Rows, Prev).
solve(pre(Atom),      _,    Prev) :- !, member(Atom, Prev).
solve(Goal,           _,    _)    :- arithmetic_goal(Goal), !, call(Goal).
solve(Atom,           Rows, _)    :- member(Atom, Rows).

arithmetic_goal(Goal) :-
    functor(Goal, Name, Arity),
    memberchk(Name/Arity, [ (is)/2, (<)/2, (>)/2, (=<)/2, (>=)/2,
                            (=:=)/2, (=\=)/2, (==)/2, (\==)/2, (=)/2 ]).

body_atoms((Left, Right), Atoms) :- !,
    body_atoms(Left, LeftAtoms), body_atoms(Right, RightAtoms),
    append(LeftAtoms, RightAtoms, Atoms).
body_atoms(pre(_),  []) :- !.
body_atoms(not(_),  []) :- !.
body_atoms(Goal,    []) :- arithmetic_goal(Goal), !.
body_atoms(Atom,    [Atom]).

% ── level rules: membership, recomputed from scratch each tick, so a body row
%    going away takes its consequences with it.
level_closure(Program, Base, Prev, Level) :- level_step(Program, Base, Prev, [], Level).

level_step(Program, Base, Prev, Known0, Level) :-
    append(Base, Known0, Visible),
    findall(Head, ( rule_of(Program, (Head <- Body)), solve(Body, Visible, Prev) ), Heads),
    append(Known0, Heads, Merged0),
    sort(Merged0, Merged),
    (   Merged == Known0
    ->  Level = Known0
    ;   level_step(Program, Base, Prev, Merged, Level) ).

% ── edge rules: fire once per (rule, arriving body atom, solution). Several
%    body atoms means several trigger positions; duplicates collapse.
edge_derivations(Program, Now, Prev, Arrived, Derived) :-
    findall(Head,
            ( rule_of(Program, (Head <+ Body)),
              body_atoms(Body, Atoms),
              member(Trigger, Atoms),
              member(Trigger, Arrived),
              solve(Body, Now, Prev) ),
            Derived0),
    dedupe_keep_order(Derived0, Derived).

% ── the keyed-rel conflict law: rules heading a keyed rel must be jointly
%    semidet per key per tick. Two DIFFERENT rows for one key = rejected.
check_key_conflicts(Program, Derived) :-
    forall(( member(Row, Derived), rel_of(Row, Ref),
             decl_key(Program, Ref, KeyPositions), key_of(KeyPositions, Row, Key) ),
           ( findall(Other, ( member(Other, Derived), rel_of(Other, Ref),
                              key_of(KeyPositions, Other, Key) ), Others0),
             sort(Others0, Others),
             (   Others = [_]
             ->  true
             ;   throw(keyed_conflict(Ref, Key, Others)) ) )).

% ── one derived row becomes one write. Keyed head + existing row for that key
%    = replacement. Derived row EQUAL to the existing row = no-op (ambiguity 1
%    in the .md; the alternative is emitting -Row/+Row).
row_write(Program, Current, Row, Write) :-
    rel_of(Row, Ref),
    (   decl_key(Program, Ref, KeyPositions)
    ->  key_of(KeyPositions, Row, Key),
        (   member(Old, Current), rel_of(Old, Ref), key_of(KeyPositions, Old, Key)
        ->  ( Old == Row -> Write = noop(Row) ; Write = replace(Old, Row) )
        ;   Write = add(Row) )
    ;   ( memberchk(Row, Current) -> Write = noop(Row) ; Write = add(Row) ) ).

apply_writes(Rows, [], Rows).
apply_writes(Rows, [add(Row) | Rest], Out) :- !,
    apply_writes([Row | Rows], Rest, Out).
apply_writes(Rows, [replace(Old, New) | Rest], Out) :- !,
    exclude(==(Old), Rows, Kept), apply_writes([New | Kept], Rest, Out).
apply_writes(Rows, [noop(_) | Rest], Out) :-
    apply_writes(Rows, Rest, Out).

apply_arrivals(Rows, [], Rows).
apply_arrivals(Rows, [Delta | Rest], Out) :-
    (   Delta = +Fact -> Next = [Fact | Rows]
    ;   Delta = -Fact, exclude(==(Fact), Rows, Next) ),
    apply_arrivals(Next, Rest, Out).

% ── THE TICK ────────────────────────────────────────────────────────────────
% Two phases, deliberately: arrivals settle the level view, edge rules fire
% against that view, writes land, the level view settles again. Not iterated
% to a joint fixpoint (ambiguity 4 in the .md).
tick(Program, StartAll, Arrivals, NextAll, Deltas) :-
    findall(Row, ( member(Row, StartAll), rel_of(Row, Ref), \+ level_rel(Program, Ref) ),
            StartPersistent),
    apply_arrivals(StartPersistent, Arrivals, WithArrivals0),
    sort(WithArrivals0, WithArrivals),
    level_closure(Program, WithArrivals, StartAll, MidLevel),
    union_rows(WithArrivals, MidLevel, MidAll),
    ord_subtract(MidAll, StartAll, Arrived),
    edge_derivations(Program, MidAll, StartAll, Arrived, Derived),
    check_key_conflicts(Program, Derived),
    maplist(row_write(Program, WithArrivals), Derived, Writes),
    apply_writes(WithArrivals, Writes, NextPersistent0),
    sort(NextPersistent0, NextPersistent),
    level_closure(Program, NextPersistent, StartAll, NextLevel),
    union_rows(NextPersistent, NextLevel, NextAll),
    row_deltas(StartAll, NextAll, Deltas).

row_deltas(Old, New, Deltas) :-
    ord_subtract(Old, New, Removed),
    ord_subtract(New, Old, Added),
    findall(-Row, member(Row, Removed), Retractions),
    findall(+Row, member(Row, Added),   Assertions),
    append(Retractions, Assertions, Deltas).

union_rows(Left, Right, Union) :- append(Left, Right, Both), sort(Both, Union).

dedupe_keep_order([], []).
dedupe_keep_order([Item | Rest], [Item | Out]) :-
    exclude(==(Item), Rest, Filtered), dedupe_keep_order(Filtered, Out).

run_trace(Program, Initial, ArrivalTicks, FinalRows, DeltaTicks) :-
    sort(Initial, Start),
    run_ticks(Program, Start, ArrivalTicks, FinalRows, DeltaTicks).

run_ticks(_, Rows, [], Rows, []).
run_ticks(Program, Rows, [Arrivals | Rest], Final, [Deltas | More]) :-
    tick(Program, Rows, Arrivals, Next, Deltas),
    run_ticks(Program, Next, Rest, Final, More).

run_named(ScenarioName, FinalRows, DeltaTicks) :-
    scenario(ScenarioName, Program, Initial, ArrivalTicks),
    run_trace(Program, Initial, ArrivalTicks, FinalRows, DeltaTicks).

% ── receipt helpers: read one rel out of a state or out of a delta trace ────
rel_rows(Ref, Rows, Selected) :-
    findall(Row, ( member(Row, Rows), rel_of(Row, Ref) ), Selected).

rel_deltas(Ref, DeltaTicks, Selected) :- maplist(rel_delta_tick(Ref), DeltaTicks, Selected).

rel_delta_tick(Ref, Deltas, Selected) :-
    findall(Delta, ( member(Delta, Deltas), delta_row(Delta, Row), rel_of(Row, Ref) ), Selected).

delta_row(+Row, Row).
delta_row(-Row, Row).

% ═══ PROGRAM 1: merge: two edge rules, one head ═══════════════════════════
% rxjs: merge(event_a$, event_b$).subscribe(out)

in_program(merge_sources).

out(Item) <+ event_a(Item).
out(Item) <+ event_b(Item).

scenario(merge_batching, merge_sources, [],
  [ [ +event_a(alpha) ]
  , [ +event_b(beta), +event_a(gamma) ]        % two sources, one tick
  , [ +event_b(delta) ]
  ]).

% ═══ PROGRAM 2: mergeByKey: the same shape, keyed head ════════════════════
% rxjs: merge(poll$, push$).pipe(scan(byKey)), but the scan disappears, the
% Key(Type) column IS last-write-per-key.

in_program(merge_by_key).

decl_of(merge_by_key, keyed(latest/2, [1])).

latest(Key, Value) <+ from_poll(Key, Value).
latest(Key, Value) <+ from_push(Key, Value).

scenario(key_last_write, merge_by_key, [],
  [ [ +from_poll(cli, v1) ]
  , [ +from_push(cli, v2) ]
  , [ +from_poll(cli, v3) ]
  ]).

scenario(key_identical_write, merge_by_key, [],
  [ [ +from_poll(cli, v1) ]
  , [ +from_push(cli, v1) ]                    % derived row EQUALS current row
  ]).

scenario(key_same_tick_conflict, merge_by_key, [],
  [ [ +from_poll(cli, v1), +from_push(cli, v2) ]
  ]).

% ═══ PROGRAM 3: mergeByKeyScan: keyed head + pre(head) ════════════════════
% redux combineReducers. Events carry an id column so two events in one tick
% are two rows and not one (set semantics would otherwise eat the second).

in_program(counter_scan).

decl_of(counter_scan, keyed(counter/2, [1])).

counter(Name, Next) <+ increment(Name, _), pre(counter(Name, Total)), Next is Total + 1.
counter(Name, Next) <+ decrement(Name, _), pre(counter(Name, Total)), Next is Total - 1.

% a level rule over the keyed state: membership, so it RETRACTS when the
% count falls back under the threshold.
hot(Name) <- counter(Name, Total), Total >= 2.

scenario(counter_fold, counter_scan, [ counter(clicks, 0) ],
  [ [ +increment(clicks, ev1) ]                % 0 -> 1
  , [ +increment(clicks, ev2) ]                % 1 -> 2
  , [ +decrement(clicks, ev3) ]                % 2 -> 1
  , [ +increment(clicks, ev4) ]                % 1 -> 2
  ]).

scenario(counter_batch_undercount, counter_scan, [ counter(clicks, 0) ],
  [ [ +increment(clicks, ev1), +increment(clicks, ev2) ]   % wanted 2, gets 1
  ]).

scenario(counter_same_tick_conflict, counter_scan, [ counter(clicks, 0) ],
  [ [ +increment(clicks, ev1), +decrement(clicks, ev2) ]
  ]).

% ═══ PROGRAM 4: seed arm and transition arm, jointly semidet ══════════════
% The negation is what discharges the conflict law: exactly one of the two
% rules can fire per key per tick.

in_program(counter_seeded).

decl_of(counter_seeded, keyed(counter/2, [1])).

counter(Name, 1)    <+ increment(Name, _), not(pre(counter(Name, _))).
counter(Name, Next) <+ increment(Name, _), pre(counter(Name, Total)), Next is Total + 1.

scenario(counter_seeds_itself, counter_seeded, [],
  [ [ +increment(clicks, ev1) ]
  , [ +increment(clicks, ev2) ]
  ]).

% ═══ PROGRAM 5: the same scan, written as one flowing cluster ═════════════
% The syntax experiment. Head rel and pre-read stated once, arms listed under
% it. Expands to program 3's two edge rules, verbatim.

in_program(counter_sugar).

decl_of(counter_sugar, keyed(counter/2, [1])).

scan(counter(Name), pre(Total),
     [ arm(increment(Name, _), Total + 1)
     , arm(decrement(Name, _), Total - 1)
     ]).

scenario(counter_sugar_fold, counter_sugar, [ counter(clicks, 0) ],
  [ [ +increment(clicks, ev1) ]
  , [ +increment(clicks, ev2) ]
  , [ +decrement(clicks, ev3) ]
  , [ +increment(clicks, ev4) ]
  ]).

% ═══ grading ═══════════════════════════════════════════════════════════════

% (a) merge lowers to two edge rules; a tick carrying arrivals from BOTH
%     sources emits both derived rows in that same tick (batching preserved).
check(merge_batches_per_tick,
  ( run_named(merge_batching, Final, DeltaTicks),
    rel_deltas(out/1, DeltaTicks, OutDeltas),
    OutDeltas == [ [ +out(alpha) ]
                 , [ +out(beta), +out(gamma) ]
                 , [ +out(delta) ] ],
    rel_rows(out/1, Final, OutRows),
    OutRows == [ out(alpha), out(beta), out(delta), out(gamma) ] )).

% edge heads never retract: nothing in the trace is a `-out(...)`.
check(merge_never_retracts,
  ( run_named(merge_batching, _, DeltaTicks),
    rel_deltas(out/1, DeltaTicks, OutDeltas),
    \+ ( member(Tick, OutDeltas), member(Delta, Tick), Delta = -_ ) )).

% (b) mergeByKey: last writer wins across ticks, and each replacement emits
%     exactly -old then +new.
check(key_last_write_wins,
  ( run_named(key_last_write, Final, DeltaTicks),
    rel_deltas(latest/2, DeltaTicks, KeyDeltas),
    KeyDeltas == [ [ +latest(cli, v1) ]
                 , [ -latest(cli, v1), +latest(cli, v2) ]
                 , [ -latest(cli, v2), +latest(cli, v3) ] ],
    rel_rows(latest/2, Final, [ latest(cli, v3) ]) )).

% the equal-row question: this interpreter says no-op, so the tick is silent.
check(key_identical_write_is_silent,
  ( run_named(key_identical_write, Final, DeltaTicks),
    rel_deltas(latest/2, DeltaTicks, KeyDeltas),
    KeyDeltas == [ [ +latest(cli, v1) ], [] ],
    rel_rows(latest/2, Final, [ latest(cli, v1) ]) )).

% (c) mergeByKeyScan: folded counter graded against hand-computed state at
%     every tick, and the level view over it retracts when the count drops.
check(counter_fold_matches_hand_computation,
  ( run_named(counter_fold, Final, DeltaTicks),
    rel_deltas(counter/2, DeltaTicks, CounterDeltas),
    CounterDeltas == [ [ -counter(clicks, 0), +counter(clicks, 1) ]
                     , [ -counter(clicks, 1), +counter(clicks, 2) ]
                     , [ -counter(clicks, 2), +counter(clicks, 1) ]
                     , [ -counter(clicks, 1), +counter(clicks, 2) ] ],
    rel_rows(counter/2, Final, [ counter(clicks, 2) ]) )).

check(level_view_retracts_over_keyed_state,
  ( run_named(counter_fold, _, DeltaTicks),
    rel_deltas(hot/1, DeltaTicks, HotDeltas),
    HotDeltas == [ [], [ +hot(clicks) ], [ -hot(clicks) ], [ +hot(clicks) ] ] )).

% seed arm and transition arm coexist under one key because the negation makes
% them mutually exclusive.
check(seed_and_transition_are_jointly_semidet,
  ( run_named(counter_seeds_itself, Final, DeltaTicks),
    rel_deltas(counter/2, DeltaTicks, CounterDeltas),
    CounterDeltas == [ [ +counter(clicks, 1) ]
                     , [ -counter(clicks, 1), +counter(clicks, 2) ] ],
    rel_rows(counter/2, Final, [ counter(clicks, 2) ]) )).

% (d) the conflict law, both shapes: two rules, one key, one tick, two
%     different rows. Rejected, with the offending rows named.
check(key_conflict_rejected,
  ( catch(run_named(key_same_tick_conflict, _, _), Thrown, true),
    Thrown == keyed_conflict(latest/2, [cli], [ latest(cli, v1), latest(cli, v2) ]) )).

check(scan_conflict_rejected,
  ( catch(run_named(counter_same_tick_conflict, _, _), Thrown, true),
    Thrown == keyed_conflict(counter/2, [clicks], [ counter(clicks, -1), counter(clicks, 1) ]) )).

% the hole the conflict law does NOT cover: two increments in one tick derive
% the SAME row, pass the law, and undercount. Graded as the defect it is.
check(scan_undercounts_batched_events,
  ( run_named(counter_batch_undercount, Final, DeltaTicks),
    rel_deltas(counter/2, DeltaTicks, CounterDeltas),
    CounterDeltas == [ [ -counter(clicks, 0), +counter(clicks, 1) ] ],
    rel_rows(counter/2, Final, [ counter(clicks, 1) ]) )).

% the syntax experiment: the cluster expands to the hand-written edge rules
% and produces a byte-identical trace.
check(scan_sugar_expands_to_plain_rules,
  ( findall(Rule, rule_of(counter_scan, (Rule)), HandRules),
    findall(Rule, rule_of(counter_sugar, (Rule)), SugarRules),
    include(is_edge_rule, HandRules, HandEdges),
    HandEdges =@= SugarRules )).

check(scan_sugar_trace_identical,
  ( run_named(counter_fold, _, HandDeltas),
    run_named(counter_sugar_fold, _, SugarDeltas),
    rel_deltas(counter/2, HandDeltas,  HandCounter),
    rel_deltas(counter/2, SugarDeltas, SugarCounter),
    HandCounter == SugarCounter )).

is_edge_rule((_ <+ _)).

go :- run(check).

% ═══ trace printer (receipts for the .md) ══════════════════════════════════

report :-
    forall(scenario(ScenarioName, Program, _, _),
           ( format("~w  (program ~w)~n", [ScenarioName, Program]),
             (   catch(run_named(ScenarioName, Final, DeltaTicks), Thrown, true)
             ->  (   var(Thrown)
                 ->  forall(nth1(Index, DeltaTicks, Deltas),
                            format("  tick ~w  ~q~n", [Index, Deltas])),
                     format("  final    ~q~n", [Final])
                 ;   format("  REJECTED ~q~n", [Thrown]) )
             ;   format("  no solution~n", []) ),
             nl )).
