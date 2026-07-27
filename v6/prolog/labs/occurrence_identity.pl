% occurrence_identity.pl : ruling R1, within-tick occurrence identity.
%
% Run:     swipl -q -l v6/prolog/labs/occurrence_identity.pl -g go -g halt
% Trace:   swipl -q -l v6/prolog/labs/occurrence_identity.pl -g report -g halt
%
% The defect (merge_family check scan_undercounts_batched_events): a tick is a
% set, so two increments arriving in one tick fold to +1 and nothing complains.
% Two candidate fixes exist and this lab runs BOTH over the SAME programs
% through ONE interpreter with a mechanism switch.
%
%   A, engine-stamped sequence : every arriving row carries an implicit
%       (tick, seq_in_tick) stamp. Folds run once per stamp, in stamp order.
%       Rows are distinct by stamp, so multiplicity AND order both survive.
%
%   B, Z-set multiplicity      : rows carry an integer count. Two identical
%       arrivals are one row with count 2. Folds are defined over
%       (row, count) pairs. There is no order inside a tick.
%
% Three more switch settings exist so the failure modes can be graded rather
% than asserted:
%
%   a_naive : mechanism A with stamps on EVERY rel, and a demand row crossing
%             the coastline once per occurrence. This is the version that
%             breaks content-addressed dedup.
%   b_naive : mechanism B where a count increment on a demand row also crosses.
%             Same disease, counted instead of stamped.
%   hybrid  : counts everywhere (so count-IVM stays native) PLUS stamps on rels
%             declared as event streams.
%
% Level rules see the SET PROJECTION under every mechanism (stamps and counts
% erased). Folds and edge rules see occurrences. That split is a decision this
% lab makes and grades, not something LANG.md states.

:- use_module('../src/grader.pl').
:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(library(ordsets)).

:- op(1150, xfx, <-).       % LEVEL rule: maintained view over membership
:- op(1150, xfx, <+).       % EDGE rule:  fires on arrivals

:- dynamic current_program/1.
:- discontiguous in_program/1, rule_of/2, decl_of/2, scenario/4, check/2.

% ═══ the reader hook (same shape as merge_family.pl) ═══════════════════════

term_expansion(in_program(Name), [in_program(Name)]) :-
    retractall(current_program(_)),
    assertz(current_program(Name)).

term_expansion((Head <+ Body), rule_of(Name, (Head <+ Body))) :- current_program(Name).
term_expansion((Head <- Body), rule_of(Name, (Head <- Body))) :- current_program(Name).

% ═══ the mechanism switch ══════════════════════════════════════════════════
% mechanism(Name, IdentityCarrier, IdentityScope, CoastlineFiring)

mechanism(a,       stamps,             event_rels, membership).
mechanism(a_naive, stamps,             all_rels,   occurrence).
mechanism(b,       counts,             all_rels,   membership).
mechanism(b_naive, counts,             all_rels,   occurrence).
mechanism(hybrid,  stamps_over_counts, event_rels, membership).

mechanism_counts(b).
mechanism_counts(b_naive).
mechanism_counts(hybrid).

carries_stamps(Mechanism, Program, Ref) :-
    mechanism(Mechanism, Carrier, Scope, _),
    memberchk(Carrier, [stamps, stamps_over_counts]),
    (   Scope == all_rels
    ->  true
    ;   decl_of(Program, event_rel(Ref)) ).

% storage cost, per rel, as columns added beyond the program's own columns.
extra_columns(Mechanism, Program, Ref, Columns) :-
    (   carries_stamps(Mechanism, Program, Ref) -> Columns = 2   % tick + seq
    ;   mechanism_counts(Mechanism)             -> Columns = 1   % count
    ;   Columns = 0 ).                                           % plain set

% ═══ rows, refs, bodies ════════════════════════════════════════════════════

row_ref(Row, Name/Arity) :- functor(Row, Name, Arity).

body_list((Left, Right), Items) :- !,
    body_list(Left, LeftItems), body_list(Right, RightItems),
    append(LeftItems, RightItems, Items).
body_list(Item, [Item]).

has_pre(Body) :- body_list(Body, Items), memberchk(pre(_), Items).

arithmetic_goal(Goal) :-
    functor(Goal, Name, Arity),
    memberchk(Name/Arity, [ (is)/2, (<)/2, (>)/2, (=<)/2, (>=)/2,
                            (=:=)/2, (=\=)/2, (==)/2, (\==)/2, (=)/2,
                            atom_concat/3 ]).

% A fold rule is an edge rule whose body reads `pre` of its own head. Its parts:
% the driving event atom (bare, or wrapped in count_of/2 for the count-aware
% form), the pre-read, and the remaining goals.
fold_parts(Program, Body, Driver, CountSlot, PreAtom, Goals) :-
    body_list(Body, Items),
    select(pre(PreAtom), Items, Rest),
    select(DriverItem, Rest, Goals),
    driver_item(Program, DriverItem, Driver, CountSlot),
    !.

driver_item(Program, count_of(Atom, CountSlot), Atom, CountSlot) :- !,
    event_atom(Program, Atom).
driver_item(Program, Atom, Atom, none) :-
    event_atom(Program, Atom).

event_atom(Program, Atom) :-
    row_ref(Atom, Ref), decl_of(Program, event_rel(Ref)).

% ═══ occurrences ═══════════════════════════════════════════════════════════
% Arrivals reach the tick as an ORDERED LIST with duplicates allowed. That is
% the honest input: the world delivers a sequence and the mechanism decides how
% much of it survives.

stamp_arrivals(_, Seq, [], [], Seq).
stamp_arrivals(TickNumber, Seq, [Row | Rest], [occ(st(TickNumber, Seq), Row) | Out], SeqOut) :-
    NextSeq is Seq + 1,
    stamp_arrivals(TickNumber, NextSeq, Rest, Out, SeqOut).

counted_occurrences(Stamped, Occurrences) :-
    findall(Row, member(occ(_, Row), Stamped), Rows),
    msort(Rows, Sorted),
    run_lengths(Sorted, Pairs),
    findall(occ(count(Count), Row), member(Row-Count, Pairs), Occurrences).

run_lengths([], []).
run_lengths([Row | Rest], [Row-Count | Pairs]) :-
    same_prefix(Row, Rest, Repeats, Tail),
    Count is Repeats + 1,
    run_lengths(Tail, Pairs).

same_prefix(Row, [Next | Rest], Repeats, Tail) :- Next == Row, !,
    same_prefix(Row, Rest, Fewer, Tail), Repeats is Fewer + 1.
same_prefix(_, Rest, 0, Rest).

fold_driver_refs(Program, Refs) :-
    findall(Ref,
            ( rule_of(Program, (_ <+ Body)), has_pre(Body),
              fold_parts(Program, Body, Driver, _, _, _),
              row_ref(Driver, Ref) ),
            Refs0),
    sort(Refs0, Refs).

% If every fold driver carries stamps the tick has an order and the fold runs
% per stamp. If any driver does not, the whole fold drops to counted mode,
% because a partial order is not an order.
fold_occurrences(Mechanism, Program, Stamped, Occurrences) :-
    fold_driver_refs(Program, Refs),
    include(occurrence_in_refs(Refs), Stamped, Selected),
    (   forall(member(Ref, Refs), carries_stamps(Mechanism, Program, Ref))
    ->  Occurrences = Selected
    ;   counted_occurrences(Selected, Occurrences) ).

occurrence_in_refs(Refs, occ(_, Row)) :- row_ref(Row, Ref), memberchk(Ref, Refs).

% ═══ the fold ══════════════════════════════════════════════════════════════
% One occurrence is one step. `pre` reads the accumulator as the previous step
% left it, so arms chain WITHIN a tick. That is the semantic change R1 buys and
% it contradicts merge_family ambiguity 5 on purpose.

apply_occurrence(Program, occ(Identity, Row), AccIn, AccOut) :-
    (   once(( rule_of(Program, Rule), fold_fires(Program, Rule, Identity, Row, AccIn, Head) ))
    ->  replace_by_key(Program, AccIn, Head, AccOut)
    ;   AccOut = AccIn ).

fold_fires(Program, (Head0 <+ Body0), Identity, Row, AccIn, Head) :-
    copy_term((Head0 <+ Body0), (Head <+ Body)),
    has_pre(Body),
    fold_parts(Program, Body, Driver, CountSlot, PreAtom, Goals),
    Driver = Row,
    bind_count(CountSlot, Identity),
    member(PreAtom, AccIn),
    maplist(call, Goals).

% The count-aware surface form binds a Count variable out of the occurrence.
% Under A each occurrence is one, so the same rule text stays correct.
bind_count(CountSlot, _)              :- CountSlot == none, !.
bind_count(CountSlot, count(Number))  :- !, CountSlot = Number.
bind_count(CountSlot, st(_, _))       :- CountSlot = 1.

key_of(Positions, Row, Key) :-
    Row =.. [_ | Args],
    findall(Column, ( member(Position, Positions), nth1(Position, Args, Column) ), Key).

replace_by_key(Program, AccIn, Head, AccOut) :-
    row_ref(Head, Ref),
    (   decl_of(Program, keyed(Ref, Positions))
    ->  key_of(Positions, Head, Key),
        exclude(shares_key(Ref, Positions, Key), AccIn, Kept),
        AccOut = [Head | Kept]
    ;   AccOut = [Head | AccIn] ).

shares_key(Ref, Positions, Key, Row) :-
    row_ref(Row, Ref), key_of(Positions, Row, Key).

% ═══ plain edge rules (no pre): one derived row per driver occurrence ═══════

plain_edge_derived(Program, Stamped, Derived) :-
    findall(Head,
            ( member(occ(_, Row), Stamped),
              rule_of(Program, (Head <+ Body)),
              \+ has_pre(Body),
              body_list(Body, [Row]) ),
            Derived).

% ═══ the store ═════════════════════════════════════════════════════════════
% held(Row, Stamps, Count). Stamps is [] when the rel carries no stamps under
% the running mechanism. Count is 1 for a plain set row.

absorb(_, _, [], Store, Store).
absorb(Mechanism, Program, [occ(Stamp, Row) | Rest], Store0, Store) :-
    row_ref(Row, Ref),
    (   selectchk(held(Row, OldStamps, OldCount), Store0, Store1)
    ->  true
    ;   Store1 = Store0, OldStamps = [], OldCount = 0 ),
    Raised is OldCount + 1,
    (   carries_stamps(Mechanism, Program, Ref)
    ->  append(OldStamps, [Stamp], NewStamps), NewCount = Raised
    ;   NewStamps = [],
        ( mechanism_counts(Mechanism) -> NewCount = Raised ; NewCount = 1 ) ),
    absorb(Mechanism, Program, Rest, [held(Row, NewStamps, NewCount) | Store1], Store).

set_projection(Store, Rows) :-
    findall(Row, member(held(Row, _, _), Store), Rows0),
    sort(Rows0, Rows).

physical_rows(Store, Ref, Total) :-
    findall(Count,
            ( member(held(Row, Stamps, _), Store), row_ref(Row, Ref),
              ( Stamps == [] -> Count = 1 ; length(Stamps, Count) ) ),
            Counts),
    sum_list(Counts, Total).

multiplicity(Store, Row, Count) :- memberchk(held(Row, _, Count), Store).

store_of_ref(Store, Ref, Selected) :-
    findall(Entry, ( member(Entry, Store), Entry = held(Row, _, _), row_ref(Row, Ref) ), Selected0),
    msort(Selected0, Selected).

stamped_rows(Store, Ref, Pairs) :-
    findall(Stamp-Row,
            ( member(held(Row, Stamps, _), Store), row_ref(Row, Ref), member(Stamp, Stamps) ),
            Pairs0),
    msort(Pairs0, Pairs).

% ═══ level rules: the SET PROJECTION only ══════════════════════════════════

level_closure(Program, Base, Level) :- level_step(Program, Base, [], Level).

level_step(Program, Base, Known, Level) :-
    append(Base, Known, Visible),
    findall(Head, ( rule_of(Program, (Head <- Body)), solve_level(Body, Visible) ), Heads),
    append(Known, Heads, Merged0),
    sort(Merged0, Merged),
    (   Merged == Known
    ->  Level = Known
    ;   level_step(Program, Base, Merged, Level) ).

solve_level((Left, Right), Rows) :- !, solve_level(Left, Rows), solve_level(Right, Rows).
solve_level(Goal, _) :- arithmetic_goal(Goal), !, call(Goal).
solve_level(Atom, Rows) :- member(Atom, Rows).

% ═══ deltas at the tick boundary (R7's contract) ═══════════════════════════

set_deltas(Old, New, Deltas) :-
    sort(Old, OldSet), sort(New, NewSet),
    ord_subtract(OldSet, NewSet, Removed),
    ord_subtract(NewSet, OldSet, Added),
    findall(-Row, member(Row, Removed), Retractions),
    findall(+Row, member(Row, Added),   Assertions),
    append(Retractions, Assertions, Deltas).

% What crosses the coastline, per rel:
%   stamped rel  -> one delta per NEW STAMP. A stamp makes the row distinct, so
%                   the second identical arrival really is a second row.
%   occurrence   -> one delta per count increment (the b_naive failure shape).
%   membership   -> set-projection diff. Identical arrivals are silent, which is
%                   what content-addressed dedup needs.
store_deltas(Mechanism, Program, Store0, Store1, Deltas) :-
    mechanism(Mechanism, _, _, Firing),
    findall(Delta,
            ( member(held(Row, NewStamps, NewCount), Store1),
              row_ref(Row, Ref),
              ( memberchk(held(Row, Found, FoundCount), Store0)
              -> OldStamps = Found, OldCount = FoundCount
              ;  OldStamps = [], OldCount = 0 ),
              (   carries_stamps(Mechanism, Program, Ref)
              ->  member(Stamp, NewStamps), \+ memberchk(Stamp, OldStamps), Delta = +Row
              ;   Firing == occurrence
              ->  Rise is NewCount - OldCount, Rise > 0, between(1, Rise, _), Delta = +Row
              ;   OldCount =:= 0, Delta = +Row ) ),
            Deltas).

% ═══ THE TICK ══════════════════════════════════════════════════════════════

tick(Mechanism, Program, TickNumber,
     state(Store0, Acc0, Level0), Arrivals,
     state(Store, Acc, Level), Deltas) :-
    stamp_arrivals(TickNumber, 1, Arrivals, StampedArrivals, NextSeq),
    plain_edge_derived(Program, StampedArrivals, Derived),
    stamp_arrivals(TickNumber, NextSeq, Derived, StampedDerived, _),
    append(StampedArrivals, StampedDerived, AllStamped),
    absorb(Mechanism, Program, AllStamped, Store0, Store1),
    msort(Store1, Store),
    fold_occurrences(Mechanism, Program, StampedArrivals, Occurrences),
    foldl(apply_occurrence(Program), Occurrences, Acc0, Acc),
    set_projection(Store, StoreRows),
    append(StoreRows, Acc, Base0), sort(Base0, Base),
    level_closure(Program, Base, Level),
    store_deltas(Mechanism, Program, Store0, Store, StoreDeltas),
    set_deltas(Acc0, Acc, AccDeltas),
    set_deltas(Level0, Level, LevelDeltas),
    append([StoreDeltas, AccDeltas, LevelDeltas], Deltas).

run_scenario(Mechanism, ScenarioName, Final, DeltaTicks) :-
    scenario(ScenarioName, Program, InitialAcc, ArrivalTicks),
    run_ticks(Mechanism, Program, 1, state([], InitialAcc, []), ArrivalTicks, Final, DeltaTicks).

run_ticks(_, _, _, State, [], State, []).
run_ticks(Mechanism, Program, TickNumber, State, [Arrivals | Rest], Final, [Deltas | More]) :-
    tick(Mechanism, Program, TickNumber, State, Arrivals, Next, Deltas),
    Following is TickNumber + 1,
    run_ticks(Mechanism, Program, Following, Next, Rest, Final, More).

final_store(state(Store, _, _), Store).
final_acc(state(_, Acc, _), Sorted) :- msort(Acc, Sorted).

% Both orderings mechanism B is allowed to pick, run over a one-tick scenario.
% If they disagree the fold is not a function of the B-state.
b_admissible_results(Mechanism, ScenarioName, Results) :-
    scenario(ScenarioName, Program, InitialAcc, [Arrivals]),
    stamp_arrivals(1, 1, Arrivals, Stamped, _),
    fold_occurrences(Mechanism, Program, Stamped, Occurrences),
    sort(2, @=<, Occurrences, Ascending),
    reverse(Ascending, Descending),
    findall(Sorted,
            ( member(Order, [Ascending, Descending]),
              foldl(apply_occurrence(Program), Order, InitialAcc, Acc),
              msort(Acc, Sorted) ),
            Results).

% ── delta readers ──────────────────────────────────────────────────────────
delta_row(+Row, Row).
delta_row(-Row, Row).

rel_deltas(Ref, DeltaTicks, Selected) :- maplist(rel_delta_tick(Ref), DeltaTicks, Selected).

rel_delta_tick(Ref, Deltas, Selected) :-
    findall(Delta, ( member(Delta, Deltas), delta_row(Delta, Row), row_ref(Row, Ref) ), Selected).

fires(DeltaTicks, Ref, Total) :-
    findall(1, ( member(Deltas, DeltaTicks), member(+Row, Deltas), row_ref(Row, Ref) ), Ones),
    length(Ones, Total).

% ═══ PROGRAM 1: the merge-lab undercount, count-blind fold ═════════════════
% No id column on the event rel. merge_family needed increment(Name, EventId)
% for two clicks to be two rows at all; occurrence identity removes that need.

in_program(counter_blind).

decl_of(counter_blind, event_rel(increment/1)).
decl_of(counter_blind, event_rel(decrement/1)).
decl_of(counter_blind, keyed(counter/2, [1])).

counter(Name, Next) <+ increment(Name), pre(counter(Name, Total)), Next is Total + 1.
counter(Name, Next) <+ decrement(Name), pre(counter(Name, Total)), Next is Total - 1.

hot(Name) <- counter(Name, Total), Total >= 2.

scenario(two_increments_one_tick, counter_blind, [ counter(clicks, 0) ],
  [ [ increment(clicks), increment(clicks) ] ]).

scenario(mixed_inc_dec_one_tick, counter_blind, [ counter(clicks, 0) ],
  [ [ increment(clicks), decrement(clicks) ] ]).

scenario(counter_across_ticks, counter_blind, [ counter(clicks, 0) ],
  [ [ increment(clicks) ]
  , [ increment(clicks) ]
  , [ decrement(clicks) ]
  ]).

% ═══ PROGRAM 2: the same fold, count-aware ═════════════════════════════════
% This is exactly what a fold has to look like under mechanism B: the driving
% atom is wrapped so the body can name the occurrence count, and the step is
% scaled by it. Under A the count is always 1, so the text stays correct.

in_program(counter_counted).

decl_of(counter_counted, event_rel(increment/1)).
decl_of(counter_counted, keyed(counter/2, [1])).

counter(Name, Next) <+ count_of(increment(Name), Count), pre(counter(Name, Total)),
                      Next is Total + Count.

scenario(two_increments_counted, counter_counted, [ counter(clicks, 0) ],
  [ [ increment(clicks), increment(clicks) ] ]).

% ═══ PROGRAM 3: an order-dependent fold, last write wins ═══════════════════

in_program(lww_stream).

decl_of(lww_stream, event_rel(set_value/2)).
decl_of(lww_stream, keyed(latest/2, [1])).

latest(Key, Value) <+ set_value(Key, Value), pre(latest(Key, _Old)).

% v2 arrives BEFORE v1, so arrival order and term order disagree.
scenario(lww_two_writes, lww_stream, [ latest(cli, v0) ],
  [ [ set_value(cli, v2), set_value(cli, v1) ] ]).

% ═══ PROGRAM 4: an order-dependent fold with no commuting law at all ═══════

in_program(concat_stream).

decl_of(concat_stream, event_rel(append_line/2)).
decl_of(concat_stream, keyed(log/2, [1])).

log(Channel, Next) <+ append_line(Channel, Piece), pre(log(Channel, SoFar)),
                      atom_concat(SoFar, Piece, Next).

scenario(concat_ab, concat_stream, [ log(main, '') ],
  [ [ append_line(main, a), append_line(main, b) ] ]).

scenario(concat_ba, concat_stream, [ log(main, '') ],
  [ [ append_line(main, b), append_line(main, a) ] ]).

% ═══ PROGRAM 5: content-addressed demand dedup ═════════════════════════════
% `stale` is an event rel. `fetch_demand` is NOT: it is a demand rel, and the
% whole point of content addressing is that the identical request does not
% cross the coastline twice.

in_program(demand_dedup).

decl_of(demand_dedup, event_rel(stale/1)).

fetch_demand(Endpoint) <+ stale(Endpoint).

scenario(repeat_demand, demand_dedup, [],
  [ [ stale('repos/cli/cli') ]
  , [ stale('repos/cli/cli') ]
  ]).

% ═══ PROGRAM 6: the JSONL stream, three lines in one tick ══════════════════
% shell_stream's ambiguity 7. `seen` is a level view over the line rel, which
% is where the set-projection decision gets tested.

in_program(jsonl_stream).

decl_of(jsonl_stream, event_rel(line/3)).

seen(Path) <- line(_StreamId, Path, _Name).

scenario(three_lines_one_tick, jsonl_stream, [],
  [ [ line(1, 'a.ts', alpha), line(1, 'a.ts', beta), line(1, 'b.ts', gamma) ] ]).

scenario(three_lines_shuffled, jsonl_stream, [],
  [ [ line(1, 'b.ts', gamma), line(1, 'a.ts', beta), line(1, 'a.ts', alpha) ] ]).

scenario(duplicate_line_one_tick, jsonl_stream, [],
  [ [ line(1, 'a.ts', alpha), line(1, 'a.ts', alpha), line(1, 'b.ts', gamma) ] ]).

scenario(duplicate_line_two_ticks, jsonl_stream, [],
  [ [ line(1, 'a.ts', alpha) ]
  , [ line(1, 'a.ts', alpha) ]
  ]).

% ═══ grading ═══════════════════════════════════════════════════════════════

% ── (a) the merge-lab undercount ───────────────────────────────────────────

check(a_ordered_fold_reaches_two,
  ( run_scenario(a, two_increments_one_tick, Final, DeltaTicks),
    final_acc(Final, [ counter(clicks, 2) ]),
    rel_deltas(counter/2, DeltaTicks, [[ -counter(clicks, 0), +counter(clicks, 2) ]]) )).

check(b_count_blind_fold_undercounts,
  ( run_scenario(b, two_increments_one_tick, Final, _),
    final_acc(Final, [ counter(clicks, 1) ]) )).

check(b_count_aware_fold_reaches_two,
  ( run_scenario(b, two_increments_counted, Final, _),
    final_acc(Final, [ counter(clicks, 2) ]) )).

% the count-aware rule text is the ONE form that is right under both switches.
check(count_aware_fold_correct_under_both,
  ( forall(member(Mechanism, [a, b, hybrid]),
           ( run_scenario(Mechanism, two_increments_counted, Final, _),
             final_acc(Final, [ counter(clicks, 2) ]) )) )).

% merge_family ambiguity 3 dissolves: increment/1 carries no event id and two
% identical arrivals are still two occurrences under both mechanisms.
check(occurrence_identity_removes_the_id_column,
  ( run_scenario(a, two_increments_one_tick, StampedFinal, _),
    final_store(StampedFinal, StampedStore),
    physical_rows(StampedStore, increment/1, 2),
    multiplicity(StampedStore, increment(clicks), 2),
    run_scenario(b, two_increments_one_tick, CountedFinal, _),
    final_store(CountedFinal, CountedStore),
    physical_rows(CountedStore, increment/1, 1),
    multiplicity(CountedStore, increment(clicks), 2) )).

% B is well defined exactly when the step functions of the tick's distinct rows
% commute. +1 and -1 do, so both admissible orders agree.
check(b_counter_fold_is_order_independent,
  ( b_admissible_results(b, mixed_inc_dec_one_tick, Results),
    Results == [ [ counter(clicks, 0) ], [ counter(clicks, 0) ] ] )).

% ── (b) an order-dependent fold ────────────────────────────────────────────

check(a_lww_follows_arrival_order,
  ( run_scenario(a, lww_two_writes, Final, _),
    final_acc(Final, [ latest(cli, v1) ]) )).           % v1 arrived last

check(b_lww_has_two_admissible_answers,
  ( b_admissible_results(b, lww_two_writes, Results),
    Results == [ [ latest(cli, v2) ], [ latest(cli, v1) ] ] )).

check(a_concat_follows_arrival_order,
  ( run_scenario(a, concat_ab, AbFinal, _),
    final_acc(AbFinal, [ log(main, ab) ]),
    run_scenario(a, concat_ba, BaFinal, _),
    final_acc(BaFinal, [ log(main, ba) ]) )).

check(b_concat_has_two_admissible_answers,
  ( b_admissible_results(b, concat_ab, Results),
    Results == [ [ log(main, ab) ], [ log(main, ba) ] ] )).

% the sharp form of the loss: two arrival sequences, one B-state, two A-results.
check(b_state_collides_on_distinct_arrival_orders,
  ( run_scenario(b, concat_ab, AbCounted, _), final_store(AbCounted, AbCountedStore),
    run_scenario(b, concat_ba, BaCounted, _), final_store(BaCounted, BaCountedStore),
    store_of_ref(AbCountedStore, append_line/2, Shared),
    store_of_ref(BaCountedStore, append_line/2, Shared),
    run_scenario(a, concat_ab, AbStamped, _), final_acc(AbStamped, AbResult),
    run_scenario(a, concat_ba, BaStamped, _), final_acc(BaStamped, BaResult),
    AbResult \== BaResult )).

% ── (c) dedup interaction ──────────────────────────────────────────────────

check(occurrence_firing_breaks_demand_dedup,
  ( forall(member(Mechanism, [a_naive, b_naive]),
           ( run_scenario(Mechanism, repeat_demand, _, DeltaTicks),
             fires(DeltaTicks, fetch_demand/1, 2) )) )).

check(membership_firing_keeps_demand_dedup,
  ( forall(member(Mechanism, [a, b, hybrid]),
           ( run_scenario(Mechanism, repeat_demand, _, DeltaTicks),
             fires(DeltaTicks, fetch_demand/1, 1),
             rel_deltas(fetch_demand/1, DeltaTicks,
                        [ [ +fetch_demand('repos/cli/cli') ], [] ]) )) )).

% ── (d) the JSONL stream ───────────────────────────────────────────────────

check(a_preserves_jsonl_arrival_order,
  ( run_scenario(a, three_lines_one_tick, Final, _),
    final_store(Final, Store),
    stamped_rows(Store, line/3, Pairs),
    Pairs == [ st(1, 1)-line(1, 'a.ts', alpha)
             , st(1, 2)-line(1, 'a.ts', beta)
             , st(1, 3)-line(1, 'b.ts', gamma) ] )).

check(b_loses_jsonl_arrival_order,
  ( run_scenario(b, three_lines_one_tick, Forward, _), final_store(Forward, ForwardStore),
    run_scenario(b, three_lines_shuffled, Reverse, _), final_store(Reverse, ReverseStore),
    store_of_ref(ForwardStore, line/3, Shared),
    store_of_ref(ReverseStore, line/3, Shared),
    run_scenario(a, three_lines_one_tick, StampedForward, _),
    final_store(StampedForward, StampedForwardStore),
    run_scenario(a, three_lines_shuffled, StampedReverse, _),
    final_store(StampedReverse, StampedReverseStore),
    store_of_ref(StampedForwardStore, line/3, ForwardStamps),
    store_of_ref(StampedReverseStore, line/3, ReverseStamps),
    ForwardStamps \== ReverseStamps )).

% ── (e) level rules over stamped or counted rels ───────────────────────────

% A level view reads the set projection, so a repeated arrival does not make it
% flicker. That is R7's boundary-diffing contract holding under every switch.
check(level_view_sees_set_projection_not_occurrences,
  ( forall(member(Mechanism, [a, a_naive, b, b_naive, hybrid]),
           ( run_scenario(Mechanism, duplicate_line_two_ticks, _, DeltaTicks),
             rel_deltas(seen/1, DeltaTicks, [ [ +seen('a.ts') ], [] ]) )) )).

check(level_view_over_folded_state_still_retracts,
  ( forall(member(Mechanism, [a, b, hybrid]),
           ( run_scenario(Mechanism, counter_across_ticks, _, DeltaTicks),
             rel_deltas(hot/1, DeltaTicks, [ [], [ +hot(clicks) ], [ -hot(clicks) ] ]) )) )).

% ── storage cost, measured on the duplicate-line tick ──────────────────────

check(storage_cost_stamps_vs_counts,
  ( run_scenario(a, duplicate_line_one_tick, StampedFinal, _),
    final_store(StampedFinal, StampedStore),
    physical_rows(StampedStore, line/3, 3),
    extra_columns(a, jsonl_stream, line/3, 2),
    extra_columns(a, demand_dedup, fetch_demand/1, 0),
    run_scenario(b, duplicate_line_one_tick, CountedFinal, _),
    final_store(CountedFinal, CountedStore),
    physical_rows(CountedStore, line/3, 2),
    extra_columns(b, jsonl_stream, line/3, 1),
    extra_columns(b, demand_dedup, fetch_demand/1, 1),
    run_scenario(hybrid, duplicate_line_one_tick, HybridFinal, _),
    final_store(HybridFinal, HybridStore),
    physical_rows(HybridStore, line/3, 3),
    extra_columns(hybrid, jsonl_stream, line/3, 2),
    extra_columns(hybrid, demand_dedup, fetch_demand/1, 1) )).

% ── the hybrid, graded on all three failing cases ──────────────────────────

check(hybrid_settles_undercount_order_and_dedup,
  ( run_scenario(hybrid, two_increments_one_tick, CounterFinal, _),
    final_acc(CounterFinal, [ counter(clicks, 2) ]),
    run_scenario(hybrid, lww_two_writes, LwwFinal, _),
    final_acc(LwwFinal, [ latest(cli, v1) ]),
    run_scenario(hybrid, concat_ab, ConcatFinal, _),
    final_acc(ConcatFinal, [ log(main, ab) ]),
    run_scenario(hybrid, repeat_demand, _, DeltaTicks),
    fires(DeltaTicks, fetch_demand/1, 1),
    % and the count column survives underneath the stamps, so count-IVM support
    % stays readable on a stamped rel.
    run_scenario(hybrid, duplicate_line_one_tick, LineFinal, _),
    final_store(LineFinal, LineStore),
    multiplicity(LineStore, line(1, 'a.ts', alpha), 2),
    stamped_rows(LineStore, line/3, Pairs),
    memberchk(st(1, 1)-line(1, 'a.ts', alpha), Pairs),
    memberchk(st(1, 2)-line(1, 'a.ts', alpha), Pairs) )).

% ── delta shape: what actually crosses the coastline ───────────────────────
% Three identical-plus-one arrivals in one tick. A ships one delta per stamp,
% B ships one per distinct row. That difference is the write volume.
check(delta_shape_measured_per_mechanism,
  ( forall(member(Mechanism-Expected, [ a-3, a_naive-3, b-2, b_naive-3, hybrid-3 ]),
           ( run_scenario(Mechanism, duplicate_line_one_tick, _, DeltaTicks),
             fires(DeltaTicks, line/3, Expected) )),
    % across ticks the same split shows up as replay versus silence
    forall(member(Mechanism2-Shape,
                  [ a-[[ +line(1, 'a.ts', alpha) ], [ +line(1, 'a.ts', alpha) ]]
                  , b-[[ +line(1, 'a.ts', alpha) ], []] ]),
           ( run_scenario(Mechanism2, duplicate_line_two_ticks, _, Ticks),
             rel_deltas(line/3, Ticks, Shape) )) )).

go :- run(check).

% ═══ trace printer (receipts for the .md) ══════════════════════════════════

report :-
    forall(member(Mechanism, [a, a_naive, b, b_naive, hybrid]),
           ( format("~n══ mechanism ~w ══~n", [Mechanism]),
             forall(scenario(ScenarioName, Program, _, _),
                    ( format("~w  (program ~w)~n", [ScenarioName, Program]),
                      (   catch(run_scenario(Mechanism, ScenarioName, Final, DeltaTicks), Thrown, true)
                      ->  (   var(Thrown)
                          ->  forall(nth1(Index, DeltaTicks, Deltas),
                                     format("  tick ~w  ~q~n", [Index, Deltas])),
                              final_store(Final, Store), final_acc(Final, Acc),
                              format("  store    ~q~n", [Store]),
                              format("  acc      ~q~n", [Acc])
                          ;   format("  THREW ~q~n", [Thrown]) )
                      ;   format("  no solution~n", []) ) )) )).
