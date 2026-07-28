% model.pl : the SMALL model interpreter for the frontier scenarios.
%
% It MODELS v6/prolog/conformance/engine.pl; it never edits or replaces it.
% Every scenario that desugars to a plain kernel program is run on the real
% oracle instead (scenarios.pl calls engine:run_program/5 read-only). This
% file exists for exactly the two things the oracle cannot express, because
% no such machinery is in it:
%
%   1. a PRIMITIVE Ta queue (dedalus async: an edge write that leaves the
%      engine and re-enters as an outside arrival some later tick).
%      engine.pl's run_ticks/7 (:367-379) has exactly two legs, scheduled
%      and drain, and no third queue anywhere.
%   2. SPILL-AT-CAP: engine.pl:373-379 throws drain_overflow(100) and offers
%      no alternative, so "does spilling change the log" is unaskable there.
%
% Modelled faithfully from engine.pl, with the citation for each choice:
%   - carry occurrences run BEFORE outside arrivals (tick/7:290, the
%     append([CarryOccs, ArrivalOccs, LevelOccs]) order).
%   - a Set arrival that is already present is NOT an occurrence
%     (absorb_arrivals:192-195).
%   - a keyed write whose row equals the stored row is a no-op with no delta
%     (apply_edge_writes:247-248, ruling r_equal_row_write).
%   - boundary deltas diff the PRE-ARRIVAL store against the post-write
%     store (tick/7:298 passes Store0, not StoreArrived).
%   - carry-out is boundary-observable writes only (tick/7:299-304), plus
%     dep(Row) for -deltas of listened rels (tick/7:307-311).
%   - drain cap 100 (engine.pl:79, tsv2 tickLoop.ts:43).
%
% DELIBERATELY NOT MODELLED (the scenarios here need none of it): level
% rules, aggregates, retention, Log rels/stamps, pre/1, now/1, negation,
% keyed_conflict. Anything needing those runs on the oracle.

:- module(mf_model, [ mrun/5, mlog_strip/3, mlog_refs/2 ]).

:- use_module(library(lists)).
:- use_module(library(apply)).

% ═══ program shape ══════════════════════════════════════════════════════════
% mprog(Decls, Rules)
%   Decls : keyed(Name/Arity, [Position...]) | listen(Name/Arity)
%   Rules : mrule(Trigger, Guards, Head, Frontier)
%     Trigger  = arr(Atom) | dep(Atom)     -- the ONE trigger item (marked)
%     Guards   = [Atom...]                 -- read from the current store
%     Head     = Atom
%     Frontier = ti | ta
%
% Policies:
%   ti_only        every rule is ti; drain cap throws drain_overflow(Cap).
%   ta_after(K)    a ta rule's head is queued and delivered as an OUTSIDE
%                  arrival at Tick+K. K is the perturbation knob: the whole
%                  point is that the engine, not the program, picks it.
%   spill_at_cap   ti rules only, but at the drain cap the ti carry is moved
%                  into the Ta queue with next-SCHEDULED-tick delivery
%                  instead of throwing.

drain_cap_model(100).

%% mrun(+Prog, +Schedule, +Policy, -Log, -Residue) is det.
%  Log     = [line(Tick, Deltas)...], Deltas = [+Row | -Row ...] sorted the
%            way engine.pl's boundary_deltas/6 orders them (removals, adds).
%  Residue = the Ta queue left undelivered when the run stopped. A nonempty
%            residue IS the silent-loss receipt for SLOT-SPILL.
mrun(Prog, Schedule, Policy, Log, Residue) :-
    mloop(Prog, Policy, mstate(1, [], [], []), Schedule, 0, Log, Residue).

mloop(_, _, mstate(_, _, [], []), [], _, [], []) :- !.
mloop(Prog, Policy, State, Schedule, Drains, Log, Residue) :-
    State = mstate(Tick, _, TiCarry, TaQueue),
    (   Schedule = [Batch | ScheduleRest]
    ->  IsDrain = false
    ;   Batch = [], ScheduleRest = [], IsDrain = true
    ),
    (   IsDrain == true, TiCarry == [], TaQueue \== [],
        \+ ta_deliverable(Policy, Tick, IsDrain, TaQueue)
    ->  % nothing left this engine can schedule: the queue is stranded.
        Log = [], Residue = TaQueue
    ;   drain_cap_model(Cap),
        (   IsDrain == true, Drains >= Cap
        ->  cap_reached(Policy, Cap, State, Log, Residue)
        ;   mtick(Prog, Policy, State, Batch, IsDrain, NextState, Deltas),
            Log = [line(Tick, Deltas) | LogRest],
            ( IsDrain == true -> NextDrains is Drains + 1 ; NextDrains = Drains ),
            mloop(Prog, Policy, NextState, ScheduleRest, NextDrains, LogRest, Residue)
        )
    ).

% ti_only is loud (engine.pl:376). spill_at_cap is quiet: the carry becomes
% a Ta queue, and with the schedule exhausted nothing ever delivers it.
cap_reached(ti_only, Cap, _, _, _) :- throw(drain_overflow(Cap)).
cap_reached(ta_after(_), Cap, _, _, _) :- throw(drain_overflow(Cap)).
cap_reached(spill_at_cap, _, mstate(_, _, TiCarry, TaQueue), [], Residue) :-
    findall(spilled(Row), member(arr(Row), TiCarry), Spilled),
    append(TaQueue, Spilled, Residue).

% ═══ one tick ═══════════════════════════════════════════════════════════════

mtick(mprog(Decls, Rules), Policy, mstate(Tick, Store0, TiCarry, TaQueue0),
      Batch, IsDrain, mstate(NextTick, Store, TiCarryOut, TaQueue), Deltas) :-
    ta_due(Policy, Tick, IsDrain, TaQueue0, Due, TaQueue1),
    append(Batch, Due, Arrivals),
    absorb(Decls, Arrivals, Store0, StoreArrived, ArrivalOccurrences),
    append(TiCarry, ArrivalOccurrences, Occurrences),
    process(Decls, Rules, Policy, Tick, Occurrences,
            StoreArrived, Store, TaQueue1, TaQueue, WrittenRows),
    set_deltas(Store0, Store, Deltas),
    findall(arr(Row),
            ( member(Row, WrittenRows), memberchk(+Row, Deltas) ),
            ArrivalCarry),
    findall(dep(Row),
            ( member(-Row, Deltas), functor(Row, Name, Arity),
              memberchk(listen(Name/Arity), Decls) ),
            DepartureCarry),
    append(ArrivalCarry, DepartureCarry, TiCarryOut0),
    dedupe(TiCarryOut0, TiCarryOut),
    NextTick is Tick + 1.

% ── Ta delivery ─────────────────────────────────────────────────────────────
% ta_after(K): queued at T, delivered at T+K, drain tick or not.
% spill_at_cap: delivered only on a tick that consumes a schedule batch
% (the "next EDB tick" reading), so an exhausted schedule strands it.

ta_due(ti_only, _, _, Queue, [], Queue).
ta_due(ta_after(_), Tick, _, Queue, Due, Rest) :-
    findall(+Row, ( member(queued(DueTick, Row), Queue), DueTick =< Tick ), Due),
    findall(queued(DueTick, Row),
            ( member(queued(DueTick, Row), Queue), DueTick > Tick ), Rest).
ta_due(spill_at_cap, _, IsDrain, Queue, Due, Rest) :-
    (   IsDrain == false
    ->  findall(+Row, member(spilled(Row), Queue), Due), Rest = []
    ;   Due = [], Rest = Queue
    ).

ta_deliverable(ti_only, _, _, _) :- fail.
% ta_after delivers on ANY tick, so a drain tick will always eventually
% reach it: the queue is never stranded under this policy.
ta_deliverable(ta_after(_), _, _, Queue) :- Queue \== [].
% spill_at_cap delivers only on a tick that consumes a schedule batch, so an
% exhausted schedule strands the queue. That is the finding, not a bug.
ta_deliverable(spill_at_cap, _, IsDrain, Queue) :- IsDrain == false, Queue \== [].

% ── arrivals (engine.pl absorb_arrivals/8) ──────────────────────────────────

absorb(_, [], Store, Store, []).
absorb(Decls, [+Row | Rest], Store0, Store, Occurrences) :- !,
    (   memberchk(Row, Store0)
    ->  Store1 = Store0, Occurrences = More
    ;   store_write(Decls, Row, Store0, Store1, _),
        Occurrences = [arr(Row) | More]
    ),
    absorb(Decls, Rest, Store1, Store, More).
absorb(Decls, [-Row | Rest], Store0, Store, Occurrences) :-
    exclude(==(Row), Store0, Store1),
    absorb(Decls, Rest, Store1, Store, Occurrences).

% ── occurrence loop (engine.pl process_occurrences/7) ───────────────────────

process(_, _, _, _, [], Store, Store, TaQueue, TaQueue, []).
process(Decls, Rules, Policy, Tick, [Occurrence | Rest],
        Store0, Store, TaQueue0, TaQueue, Written) :-
    findall(Head-Frontier,
            ( member(Rule, Rules),
              copy_term(Rule, mrule(Trigger, Guards, Head, Frontier)),
              trigger_matches(Occurrence, Trigger),
              forall_guards(Guards, Store0) ),
            Derived0),
    dedupe(Derived0, Derived),
    apply_writes(Decls, Policy, Tick, Derived,
                 Store0, Store1, TaQueue0, TaQueue1, WrittenHere),
    process(Decls, Rules, Policy, Tick, Rest,
            Store1, Store, TaQueue1, TaQueue, WrittenRest),
    append(WrittenHere, WrittenRest, Written).

trigger_matches(arr(Row), arr(Atom)) :- Atom = Row.
trigger_matches(dep(Row), dep(Atom)) :- Atom = Row.

forall_guards([], _).
% calc/1 is the model's only non-atom guard: engine.pl spells it Var := Expr
% (body.pl:103). Present so a self-feeding chain can be written without
% needing 100+ seed facts to outrun the drain cap.
forall_guards([calc(Variable := Expression) | Rest], Store) :- !,
    Value is Expression, Variable = Value, forall_guards(Rest, Store).
forall_guards([Guard | Rest], Store) :- member(Guard, Store), forall_guards(Rest, Store).

apply_writes(_, _, _, [], Store, Store, TaQueue, TaQueue, []).
apply_writes(Decls, Policy, Tick, [Row-Frontier | Rest],
             Store0, Store, TaQueue0, TaQueue, Written) :-
    (   Frontier == ta
    ->  Store1 = Store0, Written = More,
        ta_enqueue(Policy, Tick, Row, TaQueue0, TaQueue1)
    ;   TaQueue1 = TaQueue0,
        store_write(Decls, Row, Store0, Store1, Changed),
        ( Changed == true -> Written = [Row | More] ; Written = More )
    ),
    apply_writes(Decls, Policy, Tick, Rest,
                 Store1, Store, TaQueue1, TaQueue, More).

ta_enqueue(ta_after(K), Tick, Row, Queue, [queued(DueTick, Row) | Queue]) :- !,
    DueTick is Tick + K.
ta_enqueue(_, _, Row, Queue, [spilled(Row) | Queue]).

% ── the store (set rows; keyed rels replace) ────────────────────────────────

store_write(Decls, Row, Store0, Store, Changed) :-
    functor(Row, Name, Arity),
    (   memberchk(keyed(Name/Arity, Positions), Decls)
    ->  key_columns(Positions, Row, Key),
        (   member(Old, Store0), functor(Old, Name, Arity),
            key_columns(Positions, Old, Key)
        ->  (   Old == Row
            ->  Store = Store0, Changed = false
            ;   exclude(==(Old), Store0, Kept), Store = [Row | Kept], Changed = true )
        ;   Store = [Row | Store0], Changed = true )
    ;   ( memberchk(Row, Store0)
        -> Store = Store0, Changed = false
        ;  Store = [Row | Store0], Changed = true )
    ).

key_columns(Positions, Row, Key) :-
    Row =.. [_ | Args],
    findall(Column, ( member(Position, Positions), nth1(Position, Args, Column) ), Key).

% ── boundary diff (engine.pl set_diff_delta/3: removals then adds) ──────────

set_deltas(Store0, Store, Deltas) :-
    msort(Store0, Previous), msort(Store, Next),
    findall(-Row, ( member(Row, Previous), \+ memberchk(Row, Next) ), Removals),
    findall(+Row, ( member(Row, Next), \+ memberchk(Row, Previous) ), Adds),
    append(Removals, Adds, Deltas).

dedupe([], []).
dedupe([Item | Rest], [Item | Out]) :-
    exclude(==(Item), Rest, Filtered), dedupe(Filtered, Out).

% ═══ log surgery (the dissolution diff) ═════════════════════════════════════

%% mlog_strip(+Log, +Ref, -Stripped) is det.
%  Drop every delta of one rel from a tick log. The dissolution claim is
%  graded as: strip the pending rel from the encoded run's log and the
%  result is byte-identical to the primitive-Ta run's log.
mlog_strip([], _, []).
mlog_strip([line(Tick, Deltas) | Rest], Ref, [line(Tick, Kept) | More]) :-
    exclude(delta_of_ref(Ref), Deltas, Kept),
    mlog_strip(Rest, Ref, More).

delta_of_ref(Name/Arity, Delta) :-
    ( Delta = +Row ; Delta = -Row ),
    functor(Row, Name, Arity).

%% mlog_refs(+Log, -Refs) is det. Every rel that appears in a log.
mlog_refs(Log, Refs) :-
    findall(Ref,
            ( member(line(_, Deltas), Log), member(Delta, Deltas),
              ( Delta = +Row ; Delta = -Row ), functor(Row, Name, Arity),
              Ref = Name/Arity ),
            Refs0),
    sort(Refs0, Refs).
