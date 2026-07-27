% engine.pl : THE reference interpreter (AGGREGATE.md section 5b).
%
% One tick semantics, merged from the nine lab interpreters under the rulings
% in rulings.pl. The labs stay as the receipts; fixtures promoted from them
% run here, and a green run over the full corpus is the proof the lab
% sketches described one language.
%
% Run: swipl -q -l v6/prolog/conformance/go.pl -g go -g halt
%
% ── the tick, stated once ───────────────────────────────────────────────────
% 1. Outside arrivals reach the tick as an ORDERED LIST with duplicates
%    (occurrence lab: the honest input). +Row into a Log rel appends with an
%    engine stamp st(Tick, Seq); +Row into a Set rel is membership add; -Row
%    from a Log rel throws (occurrences cannot un-happen).
% 2. Trigger occurrences for edge rules = carry-in from the previous tick
%    (q4 next_tick), then outside arrivals, then level rows newly true after
%    arrivals. Each occurrence fires edge rules ONE AT A TIME in order.
% 3. A body with only/1 markers fires on marked atoms only; an unmarked body
%    keeps the any-atom rule (q6 explicit_marker; the first stage of every
%    chain keeps any-atom).
% 4. pre(Atom) reads the EVOLVING pre-state: the persistent store as writes
%    so far this tick left it, previous tick's level rows frozen. First
%    occurrence therefore reads T-1; later occurrences chain (r1 rider).
% 5. Writes: keyed head = replace (-old/+new at the boundary), equal row =
%    no-op; Log head = append with stamp; unkeyed Set head from an edge rule
%    throws edge_into_unkeyed_set. Two DIFFERENT rows for one key from ONE
%    occurrence throw keyed_conflict; across occurrences the later write is
%    the fold step (the static pairwise-disjointness law owns prevention).
% 6. Boundary deltas (r7): Log rels one +Row per new stamp (delta MULTISET);
%    Set rels and level views a set diff (removed then added). Intermediate
%    fold states are not observable (R2 rider).
% 7. Carry-out = edge-written rows + newly-true post-write level rows; they
%    are trigger occurrences for T+1. The engine self-schedules drain ticks
%    (empty outside-arrival set) while carry remains (q5 engine_owned),
%    capped so a self-feeding chain fails loudly.
% 8. now(Tick) is a kernel read of the current tick (R3), never an arrival.
% 9. Log rels require a keep/2 declaration (q10); keep(count(N)) prunes to
%    the newest N stamps at tick end; keep(all) is the explicit fixture
%    escape for unbounded history.
%
% ── aggregates (q7 bag, q9 reserved head forms, json arm) ───────────────────
% count/sum/min/max/json_array/json_object appear in level-rule head column
% positions only. Grouping is by the evaluated non-aggregate head columns;
% the bag of body derivations is the multiset aggregated over. json_array
% collects the bag in canonical (msort) order; json_object throws on one key
% with two values. Edge rules never carry aggregate heads.
%
% ── json values ─────────────────────────────────────────────────────────────
% Braces terms ARE the literal syntax ({ tag: v1 } reads as {}/1); the engine
% canonicalizes to obj(SortedPairs). decode(Expr, Pattern) is the body goal:
% object patterns are OPEN (extra keys ignored), missing key and none both
% fail a bare field pattern. json_each(List, Elem) is the element fan-out.
%
% ── out of scope here ───────────────────────────────────────────────────────
% Real effect transports (shell spawn, SSE): fixtures model bind fills as
% scheduled arrivals (the canned-rows law). Pattern/grammar matching
% (astgrep lab) is a separate reference matcher, not this file.

:- module(engine,
          [ run_fixture_checks/2, run_program/5, fixture_expectations_hold/2,
            rel_rows/3, rel_deltas/3 ]).
:- reexport(body, [json_canon/2]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(library(ordsets)).
:- use_module(library(pairs)).
:- use_module(rulings).
:- use_module(body).
:- use_module(level_eval).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

:- multifile user:fixture/5.
:- discontiguous user:fixture/5.

drain_cap(100).

% ═══ program shape ══════════════════════════════════════════════════════════
% prog(Decls, Rules)
%   Decls: kind(Ref, set|log) | keyed(Ref, Positions) | keep(Ref, Bound)
%   Rules: (Head <- Body) | (Head <+ Body)
% Bound: count(N) | all      (duration bounds arrive with the clock fixtures)


declared_kind(Decls, Ref, Kind) :- memberchk(kind(Ref, Kind), Decls).

rel_kind(Decls, _, Ref, log) :- declared_kind(Decls, Ref, log), !.
rel_kind(Decls, _, Ref, set) :- declared_kind(Decls, Ref, set), !.
rel_kind(Decls, _, Ref, set) :- memberchk(keyed(Ref, _), Decls), !.
rel_kind(_, _, _, set).

level_headed(Rules, Ref) :- member((Head <- _), Rules), rel_ref(Head, Ref), !.

decl_key(Decls, Ref, Positions) :- memberchk(keyed(Ref, Positions), Decls).

key_of(Positions, Row, Key) :-
    Row =.. [_ | Args],
    findall(Column, ( member(Position, Positions), nth1(Position, Args, Column) ), Key).

% Load-time program checks: keyed-Log exclusion, retention presence.
check_program(prog(Decls, Rules)) :-
    forall(( member(keyed(Ref, _), Decls), declared_kind(Decls, Ref, log) ),
           throw(keyed_log_rel(Ref))),
    forall(( member(kind(Ref, log), Decls), \+ memberchk(keep(Ref, _), Decls) ),
           throw(missing_retention(Ref))),
    forall(( member((Head <+ _), Rules), aggregate_head(Head, _, _) ),
           throw(aggregate_in_edge_head)),
    forall(( member((_ <- Body), Rules), body_departed_ref(Body, Ref2) ),
           throw(departed_in_level_rule(Ref2))).

% ═══ the store ══════════════════════════════════════════════════════════════
% srow(Row) for Set rels; lrow(st(Tick, Seq), Row) for Log rels. Level views
% are computed, never stored.

% Multiset: a Log rel's duplicate rows are distinct occurrences and stay
% visible (store dedup would silently re-collapse what q1 preserves).
store_rows(Store, Rows) :-
    findall(Row, ( member(Entry, Store), entry_row(Entry, Row) ), Rows0),
    msort(Rows0, Rows).

entry_row(srow(Row), Row).
entry_row(lrow(_, Row), Row).

log_stamps(Store, Ref, Stamps) :-
    findall(Stamp-Row,
            ( member(lrow(Stamp, Row), Store), rel_ref(Row, Ref) ), Stamps0),
    msort(Stamps0, Stamps).

% q6: markers narrow the trigger set; an unmarked body keeps any-atom.
% r4: departed(Atom) is a DEPARTURE trigger position; it fires on a Set/level
% row's -delta arriving as a next-tick occurrence, and is never satisfiable
% as a read (the row is gone). Items are arrival(Atom) | departure(Atom).
trigger_items(Body, Items) :-
    marked_items(Body, Marked),
    ( Marked == [] -> unmarked_items(Body, Items) ; Items = Marked ).

marked_items((Left, Right), Items) :- !,
    marked_items(Left, LeftItems), marked_items(Right, RightItems),
    append(LeftItems, RightItems, Items).
marked_items(only(departed(Atom)), [departure(Atom)]) :- !.
marked_items(only(Atom), [arrival(Atom)]) :- !.
marked_items(_, []).

unmarked_items((Left, Right), Items) :- !,
    unmarked_items(Left, LeftItems), unmarked_items(Right, RightItems),
    append(LeftItems, RightItems, Items).
unmarked_items(departed(Atom), [departure(Atom)]) :- !.
% maplist, NEVER findall: findall copies its template, which would sever the
% trigger atom from the body and let solve rejoin over the whole store.
unmarked_items(Atom, Items) :-
    body_atoms(Atom, Atoms),
    maplist(wrap_arrival, Atoms, Items).

wrap_arrival(Atom, arrival(Atom)).

% An occurrence either is a departure (dep(Row) payload) matching a
% departure item, with the ground departed goal substituted away before
% solving (the row is absent from Visible), or a plain arrival.
occurrence_trigger(dep(Row), Items, Body0, Body) :- !,
    member(departure(Atom), Items), Atom = Row,
    substitute_goal(Body0, departed(Row), Body).
occurrence_trigger(Row, Items, Body, Body) :-
    member(arrival(Atom), Items), Atom = Row.


listened_departure_refs(Rules, Refs) :-
    findall(Ref, ( member((_ <+ Body), Rules), body_departed_ref(Body, Ref) ),
            Refs0),
    sort(Refs0, Refs).

body_departed_ref((Left, Right), Ref) :-
    ( body_departed_ref(Left, Ref) ; body_departed_ref(Right, Ref) ).
body_departed_ref(only(departed(Atom)), Ref) :- rel_ref(Atom, Ref).
body_departed_ref(departed(Atom), Ref) :- rel_ref(Atom, Ref).

% ═══ arrivals ═══════════════════════════════════════════════════════════════

absorb_arrivals(_, _, [], Store, Seq, Store, Seq, []).
absorb_arrivals(Prog, Tick, [Signed | Rest], Store0, Seq0, Store, Seq, Occurrences) :-
    Prog = prog(Decls, Rules),
    (   Signed = +Row
    ->  rel_ref(Row, Ref),
        rel_kind(Decls, Rules, Ref, Kind),
        (   Kind == log
        ->  Store1 = [lrow(st(Tick, Seq0), Row) | Store0],
            Seq1 is Seq0 + 1,
            Occurrences = [occ(st(Tick, Seq0), Row) | More]
        ;   ( memberchk(srow(Row), Store0)
            -> Store1 = Store0, Occurrences = More, Seq1 = Seq0
            ;  Store1 = [srow(Row) | Store0],
               Seq1 is Seq0 + 1,
               Occurrences = [occ(st(Tick, Seq0), Row) | More] ) )
    ;   Signed = -Row,
        rel_ref(Row, Ref),
        rel_kind(Decls, Rules, Ref, Kind),
        ( Kind == log -> throw(retract_from_log(Ref)) ; true ),
        exclude(==(srow(Row)), Store0, Store1),
        Seq1 = Seq0, Occurrences = More
    ),
    absorb_arrivals(Prog, Tick, Rest, Store1, Seq1, Store, Seq, More).

% ═══ edge firing, one occurrence at a time ══════════════════════════════════

process_occurrences(_, _, _, [], Store, Store, []).
process_occurrences(Prog, Tick, Frozen, [occ(_, Payload) | Rest], Store0, Store, Written) :-
    Prog = prog(Decls, Rules),
    Frozen = frozen(MidLevel, PrevLevel),
    store_rows(Store0, StoreRows),
    append(StoreRows, MidLevel, Visible0), sort(Visible0, Visible),
    append(StoreRows, PrevLevel, PreState0), sort(PreState0, PreState),
    findall(EvaluatedHead,
            ( member((Head <+ Body), Rules),
              copy_term((Head <+ Body), (HeadCopy <+ BodyCopy)),
              trigger_items(BodyCopy, Items),
              occurrence_trigger(Payload, Items, BodyCopy, SolvableBody),
              solve(SolvableBody, ctx(Visible, PreState, Tick)),
              eval_head(HeadCopy, EvaluatedHead) ),
            Derived0),
    dedupe_keep_order(Derived0, Derived),
    check_occurrence_conflicts(Decls, Derived),
    apply_edge_writes(Prog, Tick, Derived, Store0, Store1, WrittenHere),
    process_occurrences(Prog, Tick, Frozen, Rest, Store1, Store, WrittenRest),
    append(WrittenHere, WrittenRest, Written).

check_occurrence_conflicts(Decls, Derived) :-
    forall(( member(Row, Derived), rel_ref(Row, Ref),
             decl_key(Decls, Ref, Positions), key_of(Positions, Row, Key) ),
           ( findall(Other, ( member(Other, Derived), rel_ref(Other, Ref),
                              key_of(Positions, Other, Key) ), Others0),
             sort(Others0, Others),
             ( Others = [_] -> true ; throw(keyed_conflict(Ref, Key, Others)) ) )).

apply_edge_writes(_, _, [], Store, Store, []).
apply_edge_writes(Prog, Tick, [Row | Rest], Store0, Store, Written) :-
    Prog = prog(Decls, Rules),
    rel_ref(Row, Ref),
    (   rel_kind(Decls, Rules, Ref, log)
    ->  next_seq(Store0, Tick, Seq),
        Store1 = [lrow(st(Tick, Seq), Row) | Store0],
        Written = [Row | More]
    ;   decl_key(Decls, Ref, Positions)
    ->  key_of(Positions, Row, Key),
        (   member(srow(Old), Store0), rel_ref(Old, Ref), key_of(Positions, Old, Key)
        ->  (   Old == Row
            ->  Store1 = Store0, Written = More         % equal-row write = no-op
            ;   exclude(==(srow(Old)), Store0, Kept),
                Store1 = [srow(Row) | Kept], Written = [Row | More] )
        ;   Store1 = [srow(Row) | Store0], Written = [Row | More] )
    ;   throw(edge_into_unkeyed_set(Ref))
    ),
    apply_edge_writes(Prog, Tick, Rest, Store1, Store, More).

next_seq(Store, Tick, Seq) :-
    findall(Number, member(lrow(st(Tick, Number), _), Store), Numbers),
    ( max_list(Numbers, Max) -> Seq is Max + 1 ; Seq = 1 ).

% ═══ retention (q10) ════════════════════════════════════════════════════════

apply_retention(prog(Decls, _), Store0, Store) :-
    findall(Ref-Bound, member(keep(Ref, Bound), Decls), Bounds),
    foldl(prune_rel, Bounds, Store0, Store).

prune_rel(_-all, Store, Store) :- !.
prune_rel(Ref-count(Limit), Store0, Store) :-
    log_stamps(Store0, Ref, Stamped),
    length(Stamped, Total),
    Drop is max(0, Total - Limit),
    length(Dropped, Drop), append(Dropped, _, Stamped),
    exclude(dropped_entry(Ref, Dropped), Store0, Store).

dropped_entry(Ref, Dropped, lrow(Stamp, Row)) :-
    rel_ref(Row, Ref), memberchk(Stamp-Row, Dropped).

% ═══ THE TICK ═══════════════════════════════════════════════════════════════
% state(Tick, Store, PrevLevel, PrevAll)

tick(Prog, state(Tick, Store0, PrevLevel, PrevAll), CarryIn, OutsideArrivals,
     state(NextTick, Store, Level, NextAll), CarryOut, Deltas) :-
    Prog = prog(_, Rules),
    split_rules(Rules, AggRules, PlainLevel, _),
    absorb_arrivals(Prog, Tick, OutsideArrivals, Store0, 1, StoreArrived, _, ArrivalOccs),
    store_rows(StoreArrived, MidBase),
    level_closure(PlainLevel, AggRules, MidBase, Tick, MidLevel),
    ord_subtract(MidLevel, PrevLevel, NewLevelRows),
    stamp_extra(Tick, NewLevelRows, 1000, LevelOccs),
    stamp_extra(Tick, CarryIn, 2000, CarryOccs),
    append([CarryOccs, ArrivalOccs, LevelOccs], Occurrences),
    process_occurrences(Prog, Tick, frozen(MidLevel, PrevLevel), Occurrences,
                        StoreArrived, StoreWritten, WrittenRows),
    apply_retention(Prog, StoreWritten, Store),
    store_rows(Store, FinalBase),
    level_closure(PlainLevel, AggRules, FinalBase, Tick, Level),
    ord_subtract(Level, MidLevel, PostWriteLevelRows),
    append(FinalBase, Level, NextAll0), sort(NextAll0, NextAll),
    boundary_deltas(Prog, Store0, Store, PrevAll, NextAll, Deltas),
    % R2 rider + R7: carry-out is boundary-observable writes only. A row this
    % tick wrote but that is not a +delta (intermediate fold state, net-zero
    % fold, equal-row no-op) never becomes a T+1 arrival.
    append(WrittenRows, PostWriteLevelRows, CarryCandidates0),
    dedupe_keep_order(CarryCandidates0, CarryCandidates),
    findall(Row, ( member(Row, CarryCandidates), memberchk(+Row, Deltas) ), ArrivalCarry),
    % r4: a -delta of a LISTENED rel is a departure occurrence at T+1. Only
    % rels some rule actually binds with departed/1 carry, so programs
    % without departure rules never mint drain ticks for retractions.
    listened_departure_refs(Rules, DepartureRefs),
    findall(dep(Row),
            ( member(-Row, Deltas), rel_ref(Row, DepRef), memberchk(DepRef, DepartureRefs) ),
            DepartureCarry),
    append(ArrivalCarry, DepartureCarry, CarryOut),
    NextTick is Tick + 1.

stamp_extra(_, [], _, []).
stamp_extra(Tick, [Row | Rest], Seq, [occ(st(Tick, Seq), Row) | More]) :-
    NextSeq is Seq + 1,
    stamp_extra(Tick, Rest, NextSeq, More).

% r7: Log rels emit one +Row per new stamp; everything else is a set diff of
% the full visible state (removed then added).
boundary_deltas(prog(Decls, Rules), Store0, Store, PrevAll, NextAll, Deltas) :-
    findall(Stamp-Row,
            ( member(lrow(Stamp, Row), Store),
              \+ memberchk(lrow(Stamp, Row), Store0) ),
            NewStamped0),
    msort(NewStamped0, NewStamped),
    findall(+Row, member(_-Row, NewStamped), LogAdds),
    findall(Delta,
            ( set_diff_delta(PrevAll, NextAll, Delta),
              Delta = -Row, delta_ref_is_set(Decls, Rules, Row) ),
            SetRemovals),
    findall(Delta,
            ( set_diff_delta(PrevAll, NextAll, Delta),
              Delta = +Row, delta_ref_is_set(Decls, Rules, Row) ),
            SetAdds),
    append([SetRemovals, SetAdds, LogAdds], Deltas).

set_diff_delta(PrevAll, NextAll, Delta) :-
    (   member(Row, PrevAll), \+ memberchk(Row, NextAll), Delta = -Row
    ;   member(Row, NextAll), \+ memberchk(Row, PrevAll), Delta = +Row ).

delta_ref_is_set(Decls, Rules, Row) :-
    rel_ref(Row, Ref), rel_kind(Decls, Rules, Ref, Kind), Kind == set.

% ═══ the run loop, engine-owned drains (q5) ═════════════════════════════════

run_program(Prog, Initial, Schedule, FinalAll, DeltaTicks) :-
    check_program(Prog),
    seed_store(Prog, Initial, Store0),
    Prog = prog(_, Rules),
    split_rules(Rules, AggRules, PlainLevel, _),
    store_rows(Store0, BaseRows),
    level_closure(PlainLevel, AggRules, BaseRows, 0, Level0),
    append(BaseRows, Level0, All0), sort(All0, PrevAll),
    run_ticks(Prog, state(1, Store0, Level0, PrevAll), [], Schedule, 0, FinalAll, DeltaTicks).

seed_store(prog(Decls, Rules), Initial, Store) :-
    findall(Entry,
            ( nth1(Position, Initial, Row),
              rel_ref(Row, Ref),
              ( rel_kind(Decls, Rules, Ref, log)
              -> Entry = lrow(st(0, Position), Row)
              ;  Entry = srow(Row) ) ),
            Store).

run_ticks(_, state(_, Store, Level, _), [], [], _, FinalAll, []) :- !,
    store_rows(Store, Rows),
    append(Rows, Level, FinalAll0), msort(FinalAll0, FinalAll).
run_ticks(Prog, State, Carry, [Arrivals | Schedule], Drains, FinalAll, [Deltas | More]) :- !,
    tick(Prog, State, Carry, Arrivals, NextState, NextCarry, Deltas),
    run_ticks(Prog, NextState, NextCarry, Schedule, Drains, FinalAll, More).
run_ticks(Prog, State, Carry, [], Drains, FinalAll, [Deltas | More]) :-
    Carry \== [],
    drain_cap(Cap),
    ( Drains >= Cap -> throw(drain_overflow(Cap)) ; true ),
    NextDrains is Drains + 1,
    tick(Prog, State, Carry, [], NextState, NextCarry, Deltas),
    run_ticks(Prog, NextState, NextCarry, [], NextDrains, FinalAll, More).

dedupe_keep_order([], []).
dedupe_keep_order([Item | Rest], [Item | Out]) :-
    exclude(==(Item), Rest, Filtered), dedupe_keep_order(Filtered, Out).

% ═══ fixture running ════════════════════════════════════════════════════════
% fixture(Name, prog(Decls, Rules), InitialRows, Schedule, Expectations)
% Expectations: final(Ref, SortedRows) | deltas(Ref, PerTick) | ticks(N)
%             | throws(Term)

rel_rows(Ref, Rows, Selected) :-
    findall(Row, ( member(Row, Rows), rel_ref(Row, Ref) ), Selected0),
    msort(Selected0, Selected).

rel_deltas(Ref, DeltaTicks, Selected) :- maplist(rel_delta_tick(Ref), DeltaTicks, Selected).

rel_delta_tick(Ref, Deltas, Selected) :-
    findall(Delta, ( member(Delta, Deltas), delta_row(Delta, Row), rel_ref(Row, Ref) ),
            Selected).

delta_row(+Row, Row).
delta_row(-Row, Row).

fixture_expectations_hold(Name, Expectations) :-
    user:fixture(Name, Prog, Initial, Schedule, Expectations),
    (   memberchk(throws(Expected), Expectations)
    ->  catch((run_program(Prog, Initial, Schedule, _, _), fail), Thrown, true),
        Thrown == Expected
    ;   run_program(Prog, Initial, Schedule, FinalAll, DeltaTicks),
        forall(member(Expectation, Expectations),
               expectation_holds(Expectation, FinalAll, DeltaTicks))
    ).

expectation_holds(final(Ref, Expected), FinalAll, _) :-
    rel_rows(Ref, FinalAll, Actual),
    ( Actual == Expected -> true
    ; format("    MISMATCH final ~w~n      got ~q~n      want ~q~n", [Ref, Actual, Expected]),
      fail ).
expectation_holds(deltas(Ref, Expected), _, DeltaTicks) :-
    rel_deltas(Ref, DeltaTicks, Actual),
    ( Actual == Expected -> true
    ; format("    MISMATCH deltas ~w~n      got ~q~n      want ~q~n", [Ref, Actual, Expected]),
      fail ).
expectation_holds(ticks(Expected), _, DeltaTicks) :-
    length(DeltaTicks, Actual),
    ( Actual == Expected -> true
    ; format("    MISMATCH ticks got ~w want ~w~n", [Actual, Expected]), fail ).

run_fixture_checks(Name, Goal) :-
    user:fixture(Name, _, _, _, Expectations),
    Goal = engine:fixture_expectations_hold(Name, Expectations).
