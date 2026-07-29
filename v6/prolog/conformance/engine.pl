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
% 3. Bare positive body atoms are trigger sources; latest/1 samples the
%    current visible relation without becoming a trigger source.
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
            rel_rows/3, rel_deltas/3,
            % Body traversals, exported as the characterization seam for the
            % shared-walker consolidation (rank R1). body_latest_ref/2 and
            % body_pre_ref/2 are the oracle's copies of two compiler scans;
            % the test that pins them equal reaches both sides by name.
            trigger_items/2, body_finalize_ref/2,
            body_latest_ref/2, body_pre_ref/2,
            % The oracle's load-time program gate, exported so the
            % cross_plane_check_parity unit (rank R2) can put the same prog/2
            % term through both doors and compare the two exception terms.
            check_program/1,
            % Declaration queries, exported as the declaration_query_parity
            % seam (rank R9). rel_kind/3 lost the Rules argument no clause
            % ever read.
            rel_kind/3, decl_key/3 ]).
:- reexport(body, [json_canon/2]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(library(ordsets)).
:- use_module(library(pairs)).
:- use_module('../1_expansion', [expand_program/3]).
:- use_module('../0_body_walk', [walk_body/3, body_wrapper_refs/4]).
:- use_module('../0_program_check',
              [ first_violation/3, relation_kind/3, declared_key/3 ]).
:- use_module('../0_type_plane', [ world_row_shape_violation/3 ]).
:- use_module('../1_host_expand', [prepare_program/5]).
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


% Both resolvers are shared with the compiler (rank R9 of
% plans/2026-07-29-prolog-org-review.md). This file used to carry its own
% clause-for-clause copies, and rel_kind/4's Rules argument was never read by
% any clause; the declaration_query_parity unit is the receipt.
rel_kind(Decls, Ref, Kind) :- relation_kind(Decls, Ref, Kind).

decl_key(Decls, Ref, Positions) :- declared_key(Decls, Ref, Positions).

key_of(Positions, Row, Key) :-
    Row =.. [_ | Args],
    findall(Column, ( member(Position, Positions), nth1(Position, Args, Column) ), Key).

% Load-time program checks: headed relation compatibility, body markings,
% keyed-Log exclusion, and retention presence.
%
% The trigger conditions live in 0_program_check.pl, shared with the compiler's
% supported-subset gate (rank R2 of plans/2026-07-29-prolog-org-review.md).
% What stays here is this door's ORDER and this door's exception vocabulary,
% both of which are fixture data: the oracle throws bare terms, the compiler
% wraps in unsupported_construct/1, and a program violating two classes reports
% different ones at the two doors.
% STRUCT-AS-ROWS (ruling compound_storage = struct_as_rows): the declared
% value plane is checked first, ahead of every class that reads a column type,
% and the compiler's gate opens with the same two.
engine_check_order([ type_cycle,
                     column_type_unknown,
                     keyed_level_head,
                     keyed_log_rel,
                     log_on_level_headed_rel,
                     missing_retention,
                     keep_on_non_log_rel,
                     aggregate_in_edge_head,
                     finalize_in_level_rule,
                     latest_in_level_rule,
                     pre_in_level_rule ]).

check_program(Program) :-
    engine_check_order(Order),
    (   first_violation(Program, Order, violation(Name, Payload))
    ->  engine_refusal(Name, Payload, Term),
        throw(Term)
    ;   true
    ).

engine_refusal(type_cycle,              Names, type_cycle(Names)).
engine_refusal(column_type_unknown,     Name,  column_type_unknown(Name)).
engine_refusal(keyed_level_head,        Ref,   keyed_level_head(Ref)).
engine_refusal(keyed_log_rel,           Ref-_, keyed_log_rel(Ref)).
engine_refusal(log_on_level_headed_rel, Ref,   log_on_level_headed_rel(Ref)).
engine_refusal(missing_retention,       Ref,   missing_retention(Ref)).
engine_refusal(keep_on_non_log_rel,     Ref,   keep_on_non_log_rel(Ref)).
% The oracle names this one without a reference, and always has.
engine_refusal(aggregate_in_edge_head,  _,     aggregate_in_edge_head).
engine_refusal(finalize_in_level_rule,  Ref,   finalize_in_level_rule(Ref)).
engine_refusal(latest_in_level_rule,    Ref,   latest_in_level_rule(Ref)).
engine_refusal(pre_in_level_rule,       Ref,   pre_in_level_rule(Ref)).

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

% Bare positive atoms are trigger sources; latest(Atom) is a sampled read.
% r4: finalize(Atom) is a DEPARTURE trigger position; it fires on a Set/level
% row's -delta arriving as a next-tick occurrence, and is never satisfiable
% as a read (the row is gone). Items are arrival(Atom) | departure(Atom).
% The conjunction spine comes from the shared walk (rank R1 of
% plans/2026-07-29-prolog-org-review.md); the classification stays here,
% because it is NOT the registry's.
%
% The walk must NOT descend not/1 and must NOT splice next/1 or combine: a
% negated atom is not a trigger, and next/1 or combine remain wrappers rather
% than becoming live triggers on their spliced atoms.
trigger_items(Body, Items) :-
    walk_body(Body, walk_policy(descend_not(false), splice_bare(false)),
              Events),
    trigger_items_(Events, Items).

trigger_items_([], []).
trigger_items_([event(_, _, Surface, Term) | Rest], Items) :-
    (   nonvar(Term), Term = finalize(Atom)
    ->  Items = [departure(Atom) | RestItems]
    % Bound directly out of the walked body, NEVER copied: a copy would sever
    % the trigger atom from the body and let solve rejoin over the whole store.
    ;   Surface == plain_atom
    ->  Items = [arrival(Term) | RestItems]
    ;   Items = RestItems
    ),
    trigger_items_(Rest, RestItems).

wrap_arrival(Atom, arrival(Atom)).

% An occurrence either is a departure (dep(Row) payload) matching a
% departure item, with the ground finalize goal substituted away before
% solving (the row is absent from Visible), or a plain arrival.
occurrence_trigger(dep(Row), Items, Body0, Body) :- !,
    member(departure(Atom), Items), Atom = Row,
    substitute_goal(Body0, finalize(Row), Body).
occurrence_trigger(Row, Items, Body, Body) :-
    member(arrival(Atom), Items), Atom = Row.


listened_departure_refs(Rules, Refs) :-
    findall(Ref, ( member((_ <+ Body), Rules), body_finalize_ref(Body, Ref) ),
            Refs0),
    sort(Refs0, Refs).

% All three are the shared walk under different policies (rank R1 of
% plans/2026-07-29-prolog-org-review.md). The compiler shipped its own copies
% of the latest/1 and pre/1 scans as analyze:level_body_latest_ref/2 and
% analyze:level_body_pre_ref/2; both now call this same implementation, and
% the body_walk_characterization unit asserts the two sides agree case by case.
%
% finalize/1 deliberately does NOT descend not/1, matching what this file did
% before: a negated finalize is not a departure the engine listens for.
body_finalize_ref(Body, Ref) :-
    body_wrapper_refs(Body, finalize,
                      walk_policy(descend_not(false), splice_bare(false)),
                      Ref).

body_latest_ref(Body, Ref) :-
    body_wrapper_refs(Body, latest,
                      walk_policy(descend_not(true), splice_bare(false)),
                      Ref).

body_pre_ref(Body, Ref) :-
    body_wrapper_refs(Body, pre,
                      walk_policy(descend_not(true), splice_bare(false)),
                      Ref).

% ═══ arrivals ═══════════════════════════════════════════════════════════════

absorb_arrivals(_, _, [], Store, Seq, Store, Seq, []).
absorb_arrivals(Prog, Tick, [Signed | Rest], Store0, Seq0, Store, Seq, Occurrences) :-
    Prog = prog(Decls, _),
    (   Signed = +Row
    ->  rel_ref(Row, Ref),
        rel_kind(Decls, Ref, Kind),
        (   Kind == log
        ->  Store1 = [lrow(st(Tick, Seq0), Row) | Store0],
            Seq1 is Seq0 + 1,
            Occurrences = [occ(st(Tick, Seq0), Row) | More]
        ;   absorb_set_arrival(Decls, Row, Store0, Store1, Changed),
            ( Changed == false
            -> Occurrences = More, Seq1 = Seq0
            ;  Seq1 is Seq0 + 1,
               Occurrences = [occ(st(Tick, Seq0), Row) | More] ) )
    ;   Signed = -Row,
        rel_ref(Row, Ref),
        rel_kind(Decls, Ref, Kind),
        ( Kind == log -> throw(retract_from_log(Ref)) ; true ),
        exclude(==(srow(Row)), Store0, Store1),
        Seq1 = Seq0, Occurrences = More
    ),
    absorb_arrivals(Prog, Tick, Rest, Store1, Seq1, Store, Seq, More).

absorb_set_arrival(_, Row, Store, Store, false) :-
    memberchk(srow(Row), Store),
    !.
absorb_set_arrival(Decls, Row, Store0, [srow(Row) | Kept], true) :-
    rel_ref(Row, Ref),
    decl_key(Decls, Ref, Positions),
    key_of(Positions, Row, Key),
    select(srow(Old), Store0, Kept),
    rel_ref(Old, Ref),
    key_of(Positions, Old, Key),
    !.
absorb_set_arrival(_, Row, Store, [srow(Row) | Store], true).

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
    Prog = prog(Decls, _),
    rel_ref(Row, Ref),
    (   rel_kind(Decls, Ref, log)
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
    % rels some rule actually binds with finalize/1 carry, so programs
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
boundary_deltas(prog(Decls, _), Store0, Store, PrevAll, NextAll, Deltas) :-
    findall(Stamp-Row,
            ( member(lrow(Stamp, Row), Store),
              \+ memberchk(lrow(Stamp, Row), Store0) ),
            NewStamped0),
    msort(NewStamped0, NewStamped),
    findall(+Row, member(_-Row, NewStamped), LogAdds),
    findall(Delta,
            ( set_diff_delta(PrevAll, NextAll, Delta),
              Delta = -Row, delta_ref_is_set(Decls, Row) ),
            SetRemovals),
    findall(Delta,
            ( set_diff_delta(PrevAll, NextAll, Delta),
              Delta = +Row, delta_ref_is_set(Decls, Row) ),
            SetAdds),
    append([SetRemovals, SetAdds, LogAdds], Deltas).

set_diff_delta(PrevAll, NextAll, Delta) :-
    (   member(Row, PrevAll), \+ memberchk(Row, NextAll), Delta = -Row
    ;   member(Row, NextAll), \+ memberchk(Row, PrevAll), Delta = +Row ).

delta_ref_is_set(Decls, Row) :-
    rel_ref(Row, Ref), rel_kind(Decls, Ref, Kind), Kind == set.

% ═══ the run loop, engine-owned drains (q5) ═════════════════════════════════

run_program(SugaredProg, Initial, Schedule, FinalAll, DeltaTicks) :-
    prepare_program(SugaredProg, HostProg, _, _, _),
    % Host preparation stays a PRE-PASS: it mixes syntax normalization with
    % world-plan extraction, so it does not belong in the four-phase table.
    % Everything after it runs in the declared order (1_expansion.pl).
    expand_program(HostProg, Prog, _),
    check_program(Prog),
    check_world_shapes(Prog, Initial, Schedule),
    seed_store(Prog, Initial, Store0),
    Prog = prog(_, Rules),
    split_rules(Rules, AggRules, PlainLevel, _),
    store_rows(Store0, BaseRows),
    level_closure(PlainLevel, AggRules, BaseRows, 0, Level0),
    append(BaseRows, Level0, All0), sort(All0, PrevAll),
    run_ticks(Prog, state(1, Store0, Level0, PrevAll), [], Schedule, 0, FinalAll, DeltaTicks).

% SLOT-ARRIVAL-MALFORMED (ruling compound_storage = struct_as_rows): a world
% row whose value does not match the declared struct shape is a NAMED refusal
% at the boundary. The check is decl-driven -- a row that passes runs exactly
% as it did before the type existed -- and it runs here, where both the seed
% rows and the whole schedule are in hand, rather than inside absorb_arrivals,
% so a malformed row is a load failure and never a half-applied tick.
check_world_shapes(prog(Decls, _), Initial, Schedule) :-
    append([Initial | Schedule], WorldRows),
    (   world_row_shape_violation(Decls, WorldRows, mismatch(Ref, Column, TypeName, Reason))
    ->  throw(type_arrival_shape_mismatch(Ref, Column, TypeName, Reason))
    ;   true
    ).

seed_store(prog(Decls, _), Initial, Store) :-
    findall(Entry,
            ( nth1(Position, Initial, Row),
              rel_ref(Row, Ref),
              ( rel_kind(Decls, Ref, log)
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
