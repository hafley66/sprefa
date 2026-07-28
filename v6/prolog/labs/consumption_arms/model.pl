% model.pl : the SMALL model interpreter for the collapse-logging thread.
%
% It MODELS v6/prolog/conformance/engine.pl; it never edits or replaces it.
% Every scenario whose program is expressible in the kernel runs on the REAL
% ORACLE instead (scenarios call engine:run_program/5 read-only). This file
% exists for exactly ONE thing the oracle has no machinery for:
%
%   the transition-collapse trace event required by rulings.pl
%   transition_rule_semantics. engine.pl computes the boundary diff and
%   throws away the per-key write counts that produced it, so "how many
%   occurrences collapsed into this delta" is unaskable there.
%
% Modelled faithfully from engine.pl, with the citation for each choice:
%   - carry occurrences run BEFORE outside arrivals (tick/7:290, the
%     append([CarryOccs, ArrivalOccs, LevelOccs]) order).
%   - a Set arrival already present is NOT an occurrence (:192-195); a Log
%     arrival ALWAYS is, duplicate or not (:186-189).
%   - a keyed write whose row equals the stored row is a no-op with no delta
%     (apply_edge_writes:247-248, ruling r_equal_row_write).
%   - boundary deltas diff the PRE-ARRIVAL store against the post-write
%     store (tick/7:298 passes Store0, not StoreArrived); removals then adds
%     for Set rels, one +Row per new stamp for Log rels.
%   - carry-out is boundary-observable writes only (tick/7:299-304).
%   - drain cap 100 (engine.pl:79, tsv2 tickLoop.ts:43), knob here so the
%     cap interaction can be exhibited without a 100-item corpus.
%
% DELIBERATELY NOT MODELLED: level rules, aggregates, retention, negation,
% pre/1, now/1, keyed_conflict. Every scenario needing those runs on the
% oracle. cross_check/0 in collapse.pl proves the model and the oracle agree
% on a program both express BEFORE the model is trusted for anything else.

:- module(ca_model, [ crun/6 ]).

:- use_module(library(lists)).
:- use_module(library(apply)).

% ═══ program shape ══════════════════════════════════════════════════════════
% cprog(Decls, Rules)
%   Decls : keyed(Name/Arity, [Position...]) | kind(Name/Arity, log)
%   Rules : crule(Trigger, Guards, Head)
%     Trigger = arr(Atom)   -- the marked trigger item
%     Guards  = [Atom...]   -- read from the current store
%     Head    = Atom
%
% crun(+Prog, +Initial, +Schedule, +Cap, -Log, -Collapses)
%   Log       = [line(Tick, Deltas)...]
%   Collapses = [collapse(Tick, Ref, Key, Writes, NetVisible)...]
%     Writes     = how many writes landed on that key this tick
%     NetVisible = did the boundary show a +delta for that key at all
%
% The collapse event is minted for every key written more than once in one
% tick, INCLUDING the net-zero case where the boundary shows nothing. That
% choice is graded, not assumed: see collapse.pl round 3.

crun(Prog, Initial, Schedule, Cap, Log, Collapses) :-
    seed(Prog, Initial, Store0),
    cloop(Prog, cstate(1, Store0, []), Schedule, 0, Cap, Log, Collapses).

seed(cprog(Decls, _), Initial, Store) :-
    findall(Entry,
            ( nth1(Position, Initial, Row),
              ( log_rel(Decls, Row)
              -> Entry = lrow(st(0, Position), Row)
              ;  Entry = srow(Row) ) ),
            Store).

log_rel(Decls, Row) :- functor(Row, Name, Arity), memberchk(kind(Name/Arity, log), Decls).

cloop(_, cstate(_, _, []), [], _, _, [], []) :- !.
cloop(Prog, State, Schedule, Drains, Cap, Log, Collapses) :-
    State = cstate(Tick, _, _),
    (   Schedule = [Batch | ScheduleRest]
    ->  IsDrain = false
    ;   Batch = [], ScheduleRest = [], IsDrain = true
    ),
    (   IsDrain == true, Drains >= Cap
    ->  throw(drain_overflow(Cap))
    ;   ctick(Prog, State, Batch, NextState, Deltas, Here),
        Log = [line(Tick, Deltas) | LogRest],
        append(Here, MoreCollapses, Collapses),
        ( IsDrain == true -> NextDrains is Drains + 1 ; NextDrains = Drains ),
        cloop(Prog, NextState, ScheduleRest, NextDrains, Cap, LogRest, MoreCollapses)
    ).

% ═══ one tick ═══════════════════════════════════════════════════════════════

ctick(cprog(Decls, Rules), cstate(Tick, Store0, Carry), Batch,
      cstate(NextTick, Store, CarryOut), Deltas, Collapses) :-
    absorb(Decls, Tick, Batch, Store0, 1, StoreArrived, _, ArrivalOccurrences, ArrivalWrites),
    append(Carry, ArrivalOccurrences, Occurrences),
    process(Decls, Rules, Tick, Occurrences, StoreArrived, Store, WrittenRows, RuleWrites),
    append(ArrivalWrites, RuleWrites, AllWrites),
    boundary(Decls, Store0, Store, Deltas),
    findall(Row, ( member(Row, WrittenRows), memberchk(+Row, Deltas) ), CarryOut0),
    dedupe(CarryOut0, CarryOut),
    collapses_of(Tick, Decls, AllWrites, Deltas, Collapses),
    NextTick is Tick + 1.

% ── arrivals ────────────────────────────────────────────────────────────────

absorb(_, _, [], Store, Seq, Store, Seq, [], []).
absorb(Decls, Tick, [+Row | Rest], Store0, Seq0, Store, Seq, Occurrences, Writes) :- !,
    (   log_rel(Decls, Row)
    ->  Store1 = [lrow(st(Tick, Seq0), Row) | Store0], Seq1 is Seq0 + 1,
        Occurrences = [Row | More], Writes = MoreWrites
    ;   ( memberchk(srow(Row), Store0)
        -> Store1 = Store0, Seq1 = Seq0, Occurrences = More, Writes = MoreWrites
        ;  store_write(Decls, Row, Store0, Store1, _),
           Seq1 is Seq0 + 1,
           Occurrences = [Row | More],
           ( keyed_of(Decls, Row, Ref, Key)
           -> Writes = [written(Ref, Key) | MoreWrites] ; Writes = MoreWrites ) )
    ),
    absorb(Decls, Tick, Rest, Store1, Seq1, Store, Seq, More, MoreWrites).
absorb(Decls, Tick, [-Row | Rest], Store0, Seq0, Store, Seq, Occurrences, Writes) :-
    exclude(==(srow(Row)), Store0, Store1),
    absorb(Decls, Tick, Rest, Store1, Seq0, Store, Seq, Occurrences, Writes).

% ── occurrence loop ─────────────────────────────────────────────────────────

process(_, _, _, [], Store, Store, [], []).
process(Decls, Rules, Tick, [Occurrence | Rest], Store0, Store, Written, Writes) :-
    store_rows(Store0, Visible),
    findall(Head,
            ( member(Rule, Rules),
              copy_term(Rule, crule(arr(Trigger), Guards, Head)),
              Trigger = Occurrence,
              guards_hold(Guards, Visible) ),
            Derived0),
    dedupe(Derived0, Derived),
    apply_writes(Decls, Tick, Derived, Store0, Store1, WrittenHere, WritesHere),
    process(Decls, Rules, Tick, Rest, Store1, Store, WrittenRest, WritesRest),
    append(WrittenHere, WrittenRest, Written),
    append(WritesHere, WritesRest, Writes).

guards_hold([], _).
guards_hold([Variable := Expression | Rest], Visible) :- !,
    Value is Expression, Variable = Value, guards_hold(Rest, Visible).
guards_hold([Guard | Rest], Visible) :- member(Guard, Visible), guards_hold(Rest, Visible).

apply_writes(_, _, [], Store, Store, [], []).
apply_writes(Decls, Tick, [Row | Rest], Store0, Store, Written, Writes) :-
    (   log_rel(Decls, Row)
    ->  next_seq(Store0, Tick, Seq),
        Store1 = [lrow(st(Tick, Seq), Row) | Store0],
        Written = [Row | More], Writes = MoreWrites
    ;   store_write(Decls, Row, Store0, Store1, Changed),
        ( Changed == true -> Written = [Row | More] ; Written = More ),
        ( keyed_of(Decls, Row, Ref, Key)
        -> Writes = [written(Ref, Key) | MoreWrites] ; Writes = MoreWrites )
    ),
    apply_writes(Decls, Tick, Rest, Store1, Store, More, MoreWrites).

next_seq(Store, Tick, Seq) :-
    findall(Number, member(lrow(st(Tick, Number), _), Store), Numbers),
    ( max_list(Numbers, Max) -> Seq is Max + 1 ; Seq = 1 ).

% ── the store ───────────────────────────────────────────────────────────────

store_rows(Store, Rows) :-
    findall(Row, ( member(Entry, Store), entry_row(Entry, Row) ), Rows0),
    msort(Rows0, Rows).

entry_row(srow(Row), Row).
entry_row(lrow(_, Row), Row).

keyed_of(Decls, Row, Name/Arity, Key) :-
    functor(Row, Name, Arity),
    memberchk(keyed(Name/Arity, Positions), Decls),
    key_columns(Positions, Row, Key).

key_columns(Positions, Row, Key) :-
    Row =.. [_ | Args],
    findall(Column, ( member(Position, Positions), nth1(Position, Args, Column) ), Key).

store_write(Decls, Row, Store0, Store, Changed) :-
    (   keyed_of(Decls, Row, Ref, Key)
    ->  Ref = Name/Arity,
        (   member(srow(Old), Store0), functor(Old, Name, Arity),
            keyed_of(Decls, Old, Ref, Key)
        ->  (   Old == Row
            ->  Store = Store0, Changed = false
            ;   exclude(==(srow(Old)), Store0, Kept),
                Store = [srow(Row) | Kept], Changed = true )
        ;   Store = [srow(Row) | Store0], Changed = true )
    ;   ( memberchk(srow(Row), Store0)
        -> Store = Store0, Changed = false
        ;  Store = [srow(Row) | Store0], Changed = true )
    ).

% ── boundary diff ───────────────────────────────────────────────────────────

% Log adds are ordered by STAMP, not by row term (engine.pl:322-325 msorts
% Stamp-Row pairs and then projects). Sorting the rows directly would put
% v0 before v1 where the engine puts v1 before v0, and the net-zero collapse
% scenario is exactly a case where those two orders differ.
boundary(Decls, Store0, Store, Deltas) :-
    findall(Stamp-Row,
            ( member(lrow(Stamp, Row), Store), \+ memberchk(lrow(Stamp, Row), Store0) ),
            NewStamped0),
    msort(NewStamped0, NewStamped),
    findall(+Row, member(_-Row, NewStamped), LogAdds),
    set_rows(Decls, Store0, Previous), set_rows(Decls, Store, Next),
    findall(-Row, ( member(Row, Previous), \+ memberchk(Row, Next) ), Removals),
    findall(+Row, ( member(Row, Next), \+ memberchk(Row, Previous) ), Adds),
    append([Removals, Adds, LogAdds], Deltas).

set_rows(_, Store, Rows) :-
    findall(Row, member(srow(Row), Store), Rows0), msort(Rows0, Rows).

% ── collapse events ─────────────────────────────────────────────────────────
% One event per (Ref, Key) written more than once in a tick. NetVisible says
% whether the boundary carried a +delta for that key at all: false is the
% net-zero collapse, the case where silence is most misleading.

collapses_of(Tick, Decls, Writes, Deltas, Collapses) :-
    dedupe(Writes, Distinct),
    findall(collapse(Tick, Ref, Key, Count, NetVisible),
            ( member(written(Ref, Key), Distinct),
              findall(1, member(written(Ref, Key), Writes), Ones),
              length(Ones, Count), Count > 1,
              ( key_has_plus_delta(Decls, Ref, Key, Deltas)
              -> NetVisible = true ; NetVisible = false ) ),
            Collapses).

key_has_plus_delta(Decls, Ref, Key, Deltas) :-
    member(+Row, Deltas), keyed_of(Decls, Row, Ref, Key), !.

dedupe([], []).
dedupe([Item | Rest], [Item | Out]) :-
    exclude(==(Item), Rest, Filtered), dedupe(Filtered, Out).
