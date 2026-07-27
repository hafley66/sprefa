% sub_lifetimes.pl : the subscription forest, the RUNTIME half of lifetimes.
%
% Run:     swipl -q -l v6/prolog/labs/sub_lifetimes.pl -g go -g halt
% Trace:   swipl -q -l v6/prolog/labs/sub_lifetimes.pl -g report -g halt
%
% Thesis under test (user, 2026-07-27): sets and dbs do not have lifetimes,
% rx scripts do. Rows are eternal (retention-bounded); lifetime lives ONLY in
% the subscription forest. Teardown kills the script, never the data.
%
% The conformance engine (v6/prolog/conformance/engine.pl) has no subscription
% machinery. This lab carries its own interpreter: the same tick shape
% (ordered stamped arrivals, Log/Set kinds, keyed replace, next-tick carry,
% engine-owned drains, r4 departures) plus ONE new store citizen, the sub
% forest, and ONE new tick phase, completion settling. Nothing in conformance/
% is touched; sub_lifetimes.md lists exactly what the engine must gain.
%
% The forest, as rows (ruling storage_integer_keys: integer ids only):
%
%   sub(SubId, ParentId, Target)        the script node; root parent is 0
%   sub_path(SubId, [Seg, ...])         materialized path over INT segments
%   demand(DemandId, SubId, Target, Salt)   the demand rows a subscribe plants
%
% and one engine-derived level view over them:
%
%   demanded(Target, Salt) <- demand(_, _, Target, Salt).
%
% Everything else follows from ordinary rules:
%
%   laziness   an effect fill is admitted only while a demand row for its
%              (Target, Salt) exists. Demand absent = the bind never fires.
%   teardown   delete every forest row whose path has the scope's path as a
%              prefix. Level views joined against demanded/2 lose support and
%              retract; Log rows and unscoped Set rows do not move.
%   switch_map an outer arrival deletes the children of its scope and plants
%              a new child with a fresh integer id (which is also the salt).
%   dominance  the never-timer under that scope stops because its demand row
%              is gone, not because anything unsubscribed a callback.
%   fork_join  a scope_done/1 level row makes the scope delete its own
%              subtree at tick end; the combined Log row outlives it.
%   repeat     resubscribe = a new sub id + a fresh salt = a new demanded/2
%              row = a new occurrence. Same salt = no new row = silence.

:- use_module('../src/grader.pl').
:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(library(ordsets)).

:- op(1150, xfx, <-).       % LEVEL rule: maintained view, consequences retract
:- op(1150, xfx, <+).       % EDGE rule:  fires on arrivals, appends
:- op(700,  xfx, :=).

:- discontiguous scenario/4, check/2.

drain_cap(100).
first_allocated_id(1000).   % hand-written scenarios use small ids; the engine
                            % counter starts above them so the two never clash

% ═══════════════════════════════════════════════════════════════════════════
% the store
% ═══════════════════════════════════════════════════════════════════════════
% srow(Row) for Set rels; lrow(st(Tick, Seq), Row) for Log rels. Level views
% are computed. The id counter is engine state, NOT a store row (it must not
% appear in the boundary deltas).

rel_ref(Atom, Name/Arity) :- functor(Atom, Name, Arity).

rel_kind(Decls, Ref, log) :- memberchk(kind(Ref, log), Decls), !.
rel_kind(Decls, Ref, set) :- memberchk(keyed(Ref, _), Decls), !.
rel_kind(_, _, set).

decl_key(Decls, Ref, Positions) :- memberchk(keyed(Ref, Positions), Decls).

key_of(Positions, Row, Key) :-
    Row =.. [_ | Args],
    findall(Column, ( member(Position, Positions), nth1(Position, Args, Column) ), Key).

store_rows(Store, Rows) :-
    findall(Row, ( member(Entry, Store), entry_row(Entry, Row) ), Rows0),
    msort(Rows0, Rows).

entry_row(srow(Row), Row).
entry_row(lrow(_, Row), Row).

log_stamps(Store, Ref, Stamps) :-
    findall(Stamp-Row, ( member(lrow(Stamp, Row), Store), rel_ref(Row, Ref) ), Stamps0),
    msort(Stamps0, Stamps).

check_program(prog(Decls, Rules)) :-
    forall(( member(keyed(Ref, _), Decls), memberchk(kind(Ref, log), Decls) ),
           throw(keyed_log_rel(Ref))),
    forall(( member(kind(Ref, log), Decls), \+ memberchk(keep(Ref, _), Decls) ),
           throw(missing_retention(Ref))),
    forall(( member((_ <- Body), Rules), body_departed_ref(Body, Ref2) ),
           throw(departed_in_level_rule(Ref2))).

% ═══════════════════════════════════════════════════════════════════════════
% body solving
% ═══════════════════════════════════════════════════════════════════════════
% ctx(Visible, PreState, Tick). pre/1 reads the evolving pre-state (r6).

solve(true, _) :- !.
solve((Left, Right), Ctx) :- !, solve(Left, Ctx), solve(Right, Ctx).
solve(not(Goal), Ctx) :- !, \+ solve(Goal, Ctx).
solve(only(Goal), Ctx) :- !, solve(Goal, Ctx).
solve(departed(_), _) :- !, fail.          % never satisfiable as a read
solve(pre(Atom), ctx(_, PreState, _)) :- !, member(Atom, PreState).
solve(now(Tick), ctx(_, _, Tick)) :- !.
solve((Var := Expr), _) :- !, eval_expr(Expr, Value), Var = Value.
solve(Goal, _) :- comparison_goal(Goal), !, call(Goal).
solve(Atom, ctx(Visible, _, _)) :- member(Atom, Visible).

eval_expr(Expr, Value) :- arithmetic_expr(Expr), !, Value is Expr.
eval_expr(Expr, Expr).

arithmetic_expr(Expr) :- integer(Expr), !.
arithmetic_expr(Expr) :-
    compound(Expr), functor(Expr, Name, 2), memberchk(Name, [+, -, *, //, mod]),
    ground(Expr).

comparison_goal(Goal) :-
    functor(Goal, Name, Arity),
    memberchk(Name/Arity, [ (<)/2, (>)/2, (=<)/2, (>=)/2, (=:=)/2, (=\=)/2,
                            (==)/2, (\==)/2, (=)/2 ]).

body_atoms((Left, Right), Atoms) :- !,
    body_atoms(Left, LeftAtoms), body_atoms(Right, RightAtoms),
    append(LeftAtoms, RightAtoms, Atoms).
body_atoms(only(Goal), Atoms) :- !, body_atoms(Goal, Atoms).
body_atoms(pre(_), []) :- !.
body_atoms(not(_), []) :- !.
body_atoms(now(_), []) :- !.
body_atoms(departed(_), []) :- !.
body_atoms((_ := _), []) :- !.
body_atoms(Goal, []) :- comparison_goal(Goal), !.
body_atoms(Atom, [Atom]).

% ═══════════════════════════════════════════════════════════════════════════
% level closure
% ═══════════════════════════════════════════════════════════════════════════

level_closure(Rules, Base, Tick, Level) :- level_step(Rules, Base, Tick, [], Level).

level_step(Rules, Base, Tick, Known0, Level) :-
    append(Base, Known0, Visible0), sort(Visible0, Visible),
    findall(Head,
            ( member((Head0 <- Body0), Rules),
              copy_term((Head0 <- Body0), (Head <- Body)),
              solve(Body, ctx(Visible, Base, Tick)) ),
            Heads),
    append(Known0, Heads, Merged0), sort(Merged0, Merged),
    (   Merged == Known0
    ->  Level = Known0
    ;   level_step(Rules, Base, Tick, Merged, Level) ).

% ═══════════════════════════════════════════════════════════════════════════
% the sub forest: subscribe, teardown, demand lookup
% ═══════════════════════════════════════════════════════════════════════════

sub_path_of(0, _, []) :- !.
sub_path_of(SubId, Store, Path) :- memberchk(srow(sub_path(SubId, Path)), Store).

% subscribe = inserting the sub row, its path row, and its demand rows.
% ids(SubSeq, DemandSeq): one dense integer sequence per table, the sqlite
% INTEGER PRIMARY KEY shape. Ids are never reused after a teardown.
forest_subscribe(SubId, ParentId, Target, Salt, Store0, ids(SubSeq, DemandSeq0),
                 Store, ids(SubSeq, DemandSeq)) :-
    (   sub_path_of(ParentId, Store0, ParentPath)
    ->  true
    ;   throw(subscribe_under_dead_scope(ParentId)) ),
    append(ParentPath, [SubId], Path),
    DemandSeq is DemandSeq0 + 1,
    DemandId = DemandSeq,
    Store = [ srow(sub(SubId, ParentId, Target)),
              srow(sub_path(SubId, Path)),
              srow(demand(DemandId, SubId, Target, Salt)) | Store0 ].

% teardown = range-DELETE by path prefix. Mode inclusive tears the scope down
% with its children (unsubscribe, completion); children tears the children off
% and leaves the scope standing (switch_map's next value).
forest_teardown(SubId, Mode, Store0, Store) :-
    (   sub_path_of(SubId, Store0, Path)
    ->  findall(DeadId,
                ( member(srow(sub_path(DeadId, OtherPath)), Store0),
                  append(Path, Suffix, OtherPath),
                  ( Mode == inclusive -> true ; Suffix \== [] ) ),
                DeadIds),
        exclude(forest_row_of(DeadIds), Store0, Store)
    ;   Store = Store0 ).

forest_row_of(DeadIds, srow(sub(SubId, _, _)))       :- memberchk(SubId, DeadIds).
forest_row_of(DeadIds, srow(sub_path(SubId, _)))     :- memberchk(SubId, DeadIds).
forest_row_of(DeadIds, srow(demand(_, SubId, _, _))) :- memberchk(SubId, DeadIds).

% the laziness gate: a fill lands only while its request row is alive.
demand_present(Store, Target, Salt) :-
    member(srow(demand(_, _, Target, Salt)), Store), !.

% ═══════════════════════════════════════════════════════════════════════════
% the item list: arrivals, forest ops, and world fills, in ORDER
% ═══════════════════════════════════════════════════════════════════════════
% +Row / -Row                        outside arrival
% subscribe(SubId, ParentId, T, S)   plant a scope
% unsubscribe(SubId) / complete(S)   tear the scope down (inclusive)
% fill(Target, Salt, Row)            a world/timer fill, demand-gated

apply_items(Prog, Tick, Items, Store0, Counter0, Seq0, Store, Counter, Occurrences) :-
    apply_items_(Prog, Tick, Items, Store0, Counter0, Seq0, Store, Counter, _, Occurrences).

apply_items_(_, _, [], Store, Counter, Seq, Store, Counter, Seq, []).
apply_items_(Prog, Tick, [Item | Rest], Store0, Counter0, Seq0,
             Store, Counter, Seq, Occurrences) :-
    apply_item(Prog, Tick, Item, Store0, Counter0, Seq0,
               Store1, Counter1, Seq1, OccurrencesHere),
    apply_items_(Prog, Tick, Rest, Store1, Counter1, Seq1,
                 Store, Counter, Seq, OccurrencesRest),
    append(OccurrencesHere, OccurrencesRest, Occurrences).

apply_item(Prog, Tick, +Row, Store0, Counter0, Seq0, Store, Counter, Seq, Occurrences) :- !,
    absorb_row(Prog, Tick, Row, Store0, Seq0, Store1, Seq, Occurrences),
    apply_scope_swaps(Prog, Row, Store1, Counter0, Store, Counter).
apply_item(prog(Decls, _), _Tick, -Row, Store0, Counter, Seq, Store, Counter, Seq, []) :- !,
    rel_ref(Row, Ref),
    ( rel_kind(Decls, Ref, log) -> throw(retract_from_log(Ref)) ; true ),
    exclude(==(srow(Row)), Store0, Store).
apply_item(_, _, subscribe(SubId, ParentId, Target, Salt), Store0, Counter0, Seq,
           Store, Counter, Seq, []) :- !,
    forest_subscribe(SubId, ParentId, Target, Salt, Store0, Counter0, Store, Counter).
apply_item(_, _, unsubscribe(SubId), Store0, Counter, Seq, Store, Counter, Seq, []) :- !,
    forest_teardown(SubId, inclusive, Store0, Store).
apply_item(_, _, complete(SubId), Store0, Counter, Seq, Store, Counter, Seq, []) :- !,
    forest_teardown(SubId, inclusive, Store0, Store).
apply_item(Prog, Tick, fill(Target, Salt, Row), Store0, Counter, Seq0,
           Store, Counter, Seq, Occurrences) :- !,
    (   demand_present(Store0, Target, Salt)
    ->  absorb_row(Prog, Tick, Row, Store0, Seq0, Store, Seq, Occurrences)
    ;   Store = Store0, Seq = Seq0, Occurrences = [] ).

absorb_row(prog(Decls, _), Tick, Row, Store0, Seq0, Store, Seq, Occurrences) :-
    rel_ref(Row, Ref),
    rel_kind(Decls, Ref, Kind),
    (   Kind == log
    ->  Store = [lrow(st(Tick, Seq0), Row) | Store0],
        Seq is Seq0 + 1,
        Occurrences = [occ(st(Tick, Seq0), Row)]
    ;   memberchk(srow(Row), Store0)
    ->  Store = Store0, Seq = Seq0, Occurrences = []
    ;   Store = [srow(Row) | Store0],
        Seq is Seq0 + 1,
        Occurrences = [occ(st(Tick, Seq0), Row)] ).

% switch_map, as a declaration: an arrival matching Pattern tears the children
% off ParentSubId and plants a new child scope on TargetTerm. The new sub id
% IS the salt (a fresh integer per subscription, engine-assigned).
apply_scope_swaps(prog(Decls, _), Row, Store0, Counter0, Store, Counter) :-
    findall(swap(ParentSubId, Target),
            ( member(switch_scope(Pattern0, ParentSubId, TargetTerm0), Decls),
              copy_term(Pattern0-TargetTerm0, Pattern-Target),
              Pattern = Row ),
            Swaps),
    foldl(apply_one_swap, Swaps, Store0-Counter0, Store-Counter).

apply_one_swap(swap(ParentSubId, Target), Store0-ids(SubSeq0, DemandSeq0),
               Store-Counter) :-
    forest_teardown(ParentSubId, children, Store0, Store1),
    NewSubId is SubSeq0 + 1,
    forest_subscribe(NewSubId, ParentSubId, Target, NewSubId,
                     Store1, ids(NewSubId, DemandSeq0), Store, Counter).

% ═══════════════════════════════════════════════════════════════════════════
% edge firing, one occurrence at a time
% ═══════════════════════════════════════════════════════════════════════════

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
unmarked_items(Goal, Items) :-
    body_atoms(Goal, Atoms), maplist(wrap_arrival, Atoms, Items).

wrap_arrival(Atom, arrival(Atom)).

occurrence_trigger(dep(Row), Items, Body0, Body) :- !,
    member(departure(Atom), Items), Atom = Row,
    strip_departed(Body0, Body).
occurrence_trigger(Row, Items, Body, Body) :-
    member(arrival(Atom), Items), Atom = Row.

strip_departed((Left, Right), (LeftOut, RightOut)) :- !,
    strip_departed(Left, LeftOut), strip_departed(Right, RightOut).
strip_departed(only(departed(_)), true) :- !.
strip_departed(departed(_), true) :- !.
strip_departed(Goal, Goal).

body_departed_ref((Left, Right), Ref) :- !,
    ( body_departed_ref(Left, Ref) ; body_departed_ref(Right, Ref) ).
body_departed_ref(only(departed(Atom)), Ref) :- !, rel_ref(Atom, Ref).
body_departed_ref(departed(Atom), Ref) :- rel_ref(Atom, Ref).

listened_departure_refs(Rules, Refs) :-
    findall(Ref, ( member((_ <+ Body), Rules), body_departed_ref(Body, Ref) ), Refs0),
    sort(Refs0, Refs).

process_occurrences(_, _, _, [], Store, Store, []).
process_occurrences(Prog, Tick, frozen(MidLevel, PrevLevel), [occ(_, Payload) | Rest],
                    Store0, Store, Written) :-
    Prog = prog(Decls, Rules),
    store_rows(Store0, StoreRows),
    append(StoreRows, MidLevel, Visible0), sort(Visible0, Visible),
    append(StoreRows, PrevLevel, PreState0), sort(PreState0, PreState),
    findall(Head,
            ( member((Head0 <+ Body0), Rules),
              copy_term((Head0 <+ Body0), (Head <+ Body)),
              trigger_items(Body, Items),
              occurrence_trigger(Payload, Items, Body, SolvableBody),
              solve(SolvableBody, ctx(Visible, PreState, Tick)) ),
            Derived0),
    dedupe_keep_order(Derived0, Derived),
    check_occurrence_conflicts(Decls, Derived),
    apply_edge_writes(Prog, Tick, Derived, Store0, Store1, WrittenHere),
    process_occurrences(Prog, Tick, frozen(MidLevel, PrevLevel), Rest,
                        Store1, Store, WrittenRest),
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
            ->  Store1 = Store0, Written = More
            ;   exclude(==(srow(Old)), Store0, Kept),
                Store1 = [srow(Row) | Kept], Written = [Row | More] )
        ;   Store1 = [srow(Row) | Store0], Written = [Row | More] )
    ;   throw(edge_into_unkeyed_set(Ref))
    ),
    apply_edge_writes(Prog, Tick, Rest, Store1, Store, More).

next_seq(Store, Tick, Seq) :-
    findall(Number, member(lrow(st(Tick, Number), _), Store), Numbers),
    ( max_list(Numbers, Max) -> Seq is Max + 1 ; Seq = 1 ).

% ═══════════════════════════════════════════════════════════════════════════
% retention (q10)
% ═══════════════════════════════════════════════════════════════════════════

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

% ═══════════════════════════════════════════════════════════════════════════
% THE NEW TICK PHASE: completion settling (self-teardown)
% ═══════════════════════════════════════════════════════════════════════════
% A scope_done(SubId) level row means "this scope has all its terminal rows".
% Completion IS deleting its own demand rows, so the phase tears the subtree
% down and recomputes the level views. It loops because one completion can
% make an enclosing scope's scope_done row true.

settle_completions(Prog, Tick, Store0, Store, Level) :-
    Prog = prog(_, Rules),
    store_rows(Store0, Rows),
    level_closure(Rules, Rows, Tick, Level0),
    findall(SubId, ( member(scope_done(SubId), Level0),
                     member(srow(sub(SubId, _, _)), Store0) ), Finished0),
    sort(Finished0, Finished),
    (   Finished == []
    ->  Store = Store0, Level = Level0
    ;   foldl(teardown_inclusive, Finished, Store0, Store1),
        settle_completions(Prog, Tick, Store1, Store, Level) ).

teardown_inclusive(SubId, Store0, Store) :- forest_teardown(SubId, inclusive, Store0, Store).

% ═══════════════════════════════════════════════════════════════════════════
% THE TICK
% ═══════════════════════════════════════════════════════════════════════════
% state(Tick, Store, Counter, PrevLevel, PrevAll)

tick(Prog, state(Tick, Store0, Counter0, PrevLevel, PrevAll), CarryIn, Items,
     state(NextTick, Store, Counter, Level, NextAll), CarryOut, Deltas) :-
    Prog = prog(_, Rules),
    apply_items(Prog, Tick, Items, Store0, Counter0, 1, StoreArrived, Counter, ArrivalOccs),
    store_rows(StoreArrived, MidBase),
    level_closure(Rules, MidBase, Tick, MidLevel),
    ord_subtract(MidLevel, PrevLevel, NewLevelRows),
    stamp_extra(Tick, NewLevelRows, 1000, LevelOccs),
    stamp_extra(Tick, CarryIn, 2000, CarryOccs),
    append([CarryOccs, ArrivalOccs, LevelOccs], Occurrences),
    process_occurrences(Prog, Tick, frozen(MidLevel, PrevLevel), Occurrences,
                        StoreArrived, StoreWritten, WrittenRows),
    apply_retention(Prog, StoreWritten, StorePruned),
    settle_completions(Prog, Tick, StorePruned, Store, Level),
    store_rows(Store, FinalBase),
    append(FinalBase, Level, NextAll0), sort(NextAll0, NextAll),
    boundary_deltas(Prog, Store0, Store, PrevAll, NextAll, Deltas),
    ord_subtract(Level, MidLevel, PostLevelRows),
    append(WrittenRows, PostLevelRows, CarryCandidates0),
    dedupe_keep_order(CarryCandidates0, CarryCandidates),
    findall(Row, ( member(Row, CarryCandidates), memberchk(+Row, Deltas) ), ArrivalCarry),
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

boundary_deltas(prog(Decls, _), Store0, Store, PrevAll, NextAll, Deltas) :-
    findall(Stamp-Row,
            ( member(lrow(Stamp, Row), Store), \+ memberchk(lrow(Stamp, Row), Store0) ),
            NewStamped0),
    msort(NewStamped0, NewStamped),
    findall(+Row, member(_-Row, NewStamped), LogAdds),
    findall(-Row, ( member(Row, PrevAll), \+ memberchk(Row, NextAll),
                    delta_ref_is_set(Decls, Row) ), SetRemovals),
    findall(+Row, ( member(Row, NextAll), \+ memberchk(Row, PrevAll),
                    delta_ref_is_set(Decls, Row) ), SetAdds),
    append([SetRemovals, SetAdds, LogAdds], Deltas).

% NOTE the unbound Kind: rel_kind/3's cuts sit behind head unification, so
% calling it with a bound Kind falls through to the catch-all clause and calls
% a Log rel a Set rel. Ask, then compare.
delta_ref_is_set(Decls, Row) :- rel_ref(Row, Ref), rel_kind(Decls, Ref, Kind), Kind == set.

dedupe_keep_order([], []).
dedupe_keep_order([Item | Rest], [Item | Out]) :-
    exclude(==(Item), Rest, Filtered), dedupe_keep_order(Filtered, Out).

% ═══════════════════════════════════════════════════════════════════════════
% the run loop
% ═══════════════════════════════════════════════════════════════════════════

run_program(prog(Decls, Rules0), Initial, Schedule, FinalAll, DeltaTicks) :-
    check_program(prog(Decls, Rules0)),
    Rules = [ (demanded(Target, Salt) <- demand(_DemandId, _SubId, Target, Salt)) | Rules0 ],
    Prog = prog(Decls, Rules),
    seed_store(Prog, Initial, Store0),
    store_rows(Store0, BaseRows),
    level_closure(Rules, BaseRows, 0, Level0),
    append(BaseRows, Level0, All0), sort(All0, PrevAll),
    first_allocated_id(Start),
    run_ticks(Prog, state(1, Store0, ids(Start, Start), Level0, PrevAll), [], Schedule, 0,
              FinalAll, DeltaTicks).

seed_store(prog(Decls, _), Initial, Store) :-
    findall(Entry,
            ( nth1(Position, Initial, Row), rel_ref(Row, Ref),
              ( rel_kind(Decls, Ref, log)
              -> Entry = lrow(st(0, Position), Row)
              ;  Entry = srow(Row) ) ),
            Store).

run_ticks(_, state(_, Store, _, Level, _), [], [], _, FinalAll, []) :- !,
    store_rows(Store, Rows),
    append(Rows, Level, FinalAll0), msort(FinalAll0, FinalAll).
run_ticks(Prog, State, Carry, [Items | Schedule], Drains, FinalAll, [Deltas | More]) :- !,
    tick(Prog, State, Carry, Items, NextState, NextCarry, Deltas),
    run_ticks(Prog, NextState, NextCarry, Schedule, Drains, FinalAll, More).
run_ticks(Prog, State, Carry, [], Drains, FinalAll, [Deltas | More]) :-
    Carry \== [],
    drain_cap(Cap),
    ( Drains >= Cap -> throw(drain_overflow(Cap)) ; true ),
    NextDrains is Drains + 1,
    tick(Prog, State, Carry, [], NextState, NextCarry, Deltas),
    run_ticks(Prog, NextState, NextCarry, [], NextDrains, FinalAll, More).

% ═══════════════════════════════════════════════════════════════════════════
% reading the results
% ═══════════════════════════════════════════════════════════════════════════

run_named(Name, FinalAll, DeltaTicks) :-
    scenario(Name, Prog, Initial, Schedule),
    run_program(Prog, Initial, Schedule, FinalAll, DeltaTicks).

run_prefix(Name, Length, FinalAll, DeltaTicks) :-
    scenario(Name, Prog, Initial, Schedule),
    length(Prefix, Length), append(Prefix, _, Schedule),
    run_program(Prog, Initial, Prefix, FinalAll, DeltaTicks).

rel_rows(Ref, Rows, Selected) :-
    findall(Row, ( member(Row, Rows), rel_ref(Row, Ref) ), Selected0),
    msort(Selected0, Selected).

rel_deltas(Ref, DeltaTicks, Selected) :- maplist(rel_delta_tick(Ref), DeltaTicks, Selected).

rel_delta_tick(Ref, Deltas, Selected) :-
    findall(Delta, ( member(Delta, Deltas), delta_row(Delta, Row), rel_ref(Row, Ref) ),
            Selected).

delta_row(+Row, Row).
delta_row(-Row, Row).

% the whole forest, as the observer sees it
forest_rows(FinalAll, Forest) :-
    findall(Row,
            ( member(Row, FinalAll),
              rel_ref(Row, Ref),
              memberchk(Ref, [sub/3, sub_path/2, demand/4]) ),
            Forest0),
    msort(Forest0, Forest).

% fold a delta stream back into a rowset (the "only deltas cross" proof)
fold_delta_stream(PerTick, Rows) :- foldl(fold_delta_tick, PerTick, [], Rows0), msort(Rows0, Rows).

fold_delta_tick(Deltas, Rows0, Rows) :- foldl(fold_one_delta, Deltas, Rows0, Rows).

fold_one_delta(-Row, Rows0, Rows) :- !, exclude(==(Row), Rows0, Rows).
fold_one_delta(+Row, Rows0, [Row | Rows0]).

all_integers(Terms) :- forall(member(Term, Terms), integer(Term)).

% ═══════════════════════════════════════════════════════════════════════════
% SCENARIOS
% ═══════════════════════════════════════════════════════════════════════════

% ── 1. laziness: the timer fires only while demanded ────────────────────────
% The timer is canned arrivals (the bind-at-link law: canned rows are program
% text identical). The interpreter admits one only while a demand row for its
% (Target, Salt) exists, which IS the magic-set demand check.

timer_program(
  prog([ kind(tick_row/2, log), keep(tick_row/2, all) ],
       [ (live_tick(Count) <- demanded(clock5, _), tick_row(clock5, Count)) ])).

scenario(timer_never_subscribed, Prog, [],
  [ [ fill(clock5, 7, tick_row(clock5, 1)) ],
    [ fill(clock5, 7, tick_row(clock5, 2)) ],
    [ fill(clock5, 7, tick_row(clock5, 3)) ] ]) :- timer_program(Prog).

scenario(timer_demanded_then_torn_down, Prog, [],
  [ [ subscribe(1, 0, clock5, 7), fill(clock5, 7, tick_row(clock5, 1)) ],
    [ fill(clock5, 7, tick_row(clock5, 2)) ],
    [ unsubscribe(1), fill(clock5, 7, tick_row(clock5, 3)) ],
    [ fill(clock5, 7, tick_row(clock5, 4)) ] ]) :- timer_program(Prog).

% same tick, opposite ORDER: the fill lands, then the scope dies.
scenario(timer_fill_before_unsubscribe, Prog, [],
  [ [ subscribe(1, 0, clock5, 7), fill(clock5, 7, tick_row(clock5, 1)) ],
    [ fill(clock5, 7, tick_row(clock5, 2)), unsubscribe(1) ],
    [ fill(clock5, 7, tick_row(clock5, 3)) ] ]) :- timer_program(Prog).

scenario(subscribe_under_dead_scope, Prog, [],
  [ [ subscribe(1, 0, clock5, 7) ],
    [ unsubscribe(1) ],
    [ subscribe(2, 1, clock5, 8) ] ]) :- timer_program(Prog).

% ── 2. teardown: script dies, data does not ─────────────────────────────────

scenario(teardown_keeps_history,
  prog([ kind(sample/2, log),     keep(sample/2, all),
         kind(sample_log/1, log), keep(sample_log/1, all) ],
       [ (live_sample(Value) <- demanded(sensor, _), sample(sensor, Value)),
         (sample_log(Value) <+ only(sample(sensor, Value))) ]),
  [],
  [ [ subscribe(1, 0, sensor, 7), fill(sensor, 7, sample(sensor, 11)) ],
    [ fill(sensor, 7, sample(sensor, 12)) ],
    [ unsubscribe(1) ],
    [ fill(sensor, 7, sample(sensor, 13)) ] ]).

% ── 3. switch_map: scope constructor ────────────────────────────────────────
% switch_scope(Pattern, ParentSubId, Target): an arrival matching Pattern
% deletes ParentSubId's children and plants a new child on Target.

switch_program(
  prog([ kind(select/1, log), keep(select/1, all),
         kind(detail_row/2, set),
         switch_scope(select(ItemId), 1, detail(ItemId)) ],
       [ (detail_view(ItemId, Field) <- demanded(detail(ItemId), _),
                                        detail_row(ItemId, Field)) ])).

scenario(switch_map_swaps_scopes, Prog, [],
  [ [ subscribe(1, 0, panel, 7) ],
    [ +select(item_a) ],
    [ fill(detail(item_a), 1001, detail_row(item_a, body_a)) ],
    [ +select(item_b) ],
    [ fill(detail(item_a), 1001, detail_row(item_a, late_body)),
      fill(detail(item_b), 1002, detail_row(item_b, body_b)) ] ]) :- switch_program(Prog).

% the salt is what makes the stale fill stale: item_a is demanded AGAIN under
% a new scope (salt 1003), and the in-flight fill addressed to salt 1001 is
% still dropped.
scenario(switch_map_resubscribed_target, Prog, [],
  [ [ subscribe(1, 0, panel, 7) ],
    [ +select(item_a) ],
    [ +select(item_b) ],
    [ +select(item_a) ],
    [ fill(detail(item_a), 1001, detail_row(item_a, stale_body)) ],
    [ fill(detail(item_a), 1003, detail_row(item_a, fresh_body)) ] ]) :- switch_program(Prog).

% ── 4. dominance made operational ───────────────────────────────────────────
% every(300s) alone is `never`; under a switch_map scope it is
% until(outer_next / outer_complete). Same program, same canned timer, two
% subscriptions: one dominated, one not.

poll_program(
  prog([ kind(open/1, log), keep(open/1, all),
         kind(poll/2, log), keep(poll/2, all),
         switch_scope(open(Session), 1, every300(Session)) ],
       [])).

scenario(dominated_timer, Prog, [],
  [ [ subscribe(1, 0, outer, 7) ],
    [ +open(session_one) ],
    [ fill(every300(session_one), 1001, poll(session_one, 1)) ],
    [ fill(every300(session_one), 1001, poll(session_one, 2)) ],
    [ complete(1) ],
    [ fill(every300(session_one), 1001, poll(session_one, 3)) ],
    [ fill(every300(session_one), 1001, poll(session_one, 4)) ] ]) :- poll_program(Prog).

% the SAME timer subscribed at the root: nothing dominates it, so completing
% the unrelated outer scope changes nothing.
scenario(undominated_timer, Prog, [],
  [ [ subscribe(1, 0, outer, 7), subscribe(2, 0, every300(session_one), 1001) ],
    [ +open(session_one) ],
    [ fill(every300(session_one), 1001, poll(session_one, 1)) ],
    [ fill(every300(session_one), 1001, poll(session_one, 2)) ],
    [ complete(1) ],
    [ fill(every300(session_one), 1001, poll(session_one, 3)) ],
    [ fill(every300(session_one), 1001, poll(session_one, 4)) ] ]) :- poll_program(Prog).

% ── 5. forkJoin: a scope that completes itself ──────────────────────────────
% combined is an EDGE write into a Log rel, so it is an occurrence and cannot
% retract. fork_view is the same join as a LEVEL view under the scope, and it
% dies with the scope. The pair is the thesis, stated twice in one program.

fork_program(
  prog([ kind(result_a/1, log),  keep(result_a/1, all),
         kind(result_b/1, log),  keep(result_b/1, all),
         kind(combined/2, log),  keep(combined/2, all) ],
       [ (combined(ValueA, ValueB) <+ result_a(ValueA), result_b(ValueB)),
         (fork_view(ValueA, ValueB) <- demanded(arm_a, _), combined(ValueA, ValueB)),
         (scope_done(1) <- combined(_, _)) ])).

scenario(fork_join_self_teardown, Prog, [],
  [ [ subscribe(1, 0, fork, 7), subscribe(2, 1, arm_a, 8), subscribe(3, 1, arm_b, 9) ],
    [ fill(arm_a, 8, result_a(alpha)) ],
    [ fill(arm_b, 9, result_b(beta)) ],
    [ fill(arm_a, 8, result_a(late_alpha)) ] ]) :- fork_program(Prog).

% ── 6. repeat: resubscribe-on-complete ──────────────────────────────────────
% fire_log fires on demanded/2 becoming newly true, so it is the receipt for
% "the effect actually re-fired". Iteration two = new sub id + fresh salt.

repeat_program(
  prog([ kind(job_done/1, log), keep(job_done/1, all),
         kind(fire_log/2, log), keep(fire_log/2, all) ],
       [ (fire_log(Target, Salt) <+ only(demanded(Target, Salt))) ])).

scenario(repeat_fresh_salt, Prog, [],
  [ [ subscribe(1, 0, job, 101) ],
    [ fill(job, 101, job_done(1)) ],
    [ unsubscribe(1), subscribe(2, 0, job, 102) ],
    [ fill(job, 102, job_done(2)) ] ]) :- repeat_program(Prog).

% the same target with the SAME salt while the first demand row still exists:
% demanded(job, 101) is already true, so nothing arrives and nothing fires.
scenario(repeat_same_salt_is_silent, Prog, [],
  [ [ subscribe(1, 0, job, 101) ],
    [ subscribe(2, 0, job, 101) ],
    [ fill(job, 101, job_done(1)) ] ]) :- repeat_program(Prog).

% ── 7. the UI worked example ────────────────────────────────────────────────
% A list panel subscribes to a filtered/mapped level view. Selecting an item
% switch_maps a detail pane. Selecting another swaps the pane. Closing the
% panel tears everything down. The db keeps every row written on the way.

ui_program(
  prog([ kind(item/3, set),
         kind(select/1, log),   keep(select/1, all),
         kind(detail_row/2, set),
         kind(ui_log/2, log),   keep(ui_log/2, all),
         kind(closed/1, log),   keep(closed/1, all),
         switch_scope(select(ItemId), 1, detail(ItemId)) ],
       [ (visible_item(ItemId, Label) <- demanded(item_list, _),
                                         item(ItemId, Label, active)),
         (detail_view(ItemId, Field) <- demanded(detail(ItemId), _),
                                        detail_row(ItemId, Field)),
         (ui_log(selected, ItemId) <+ only(select(ItemId))),
         (closed(ItemId) <+ only(departed(visible_item(ItemId, _)))) ])).

scenario(ui_panel_session, Prog,
  [ item(item_a, alpha, active), item(item_b, beta, active),
    item(item_z, zeta, archived) ],
  [ [ subscribe(1, 0, item_list, 7) ],
    [ +item(item_c, gamma, active) ],
    [ +select(item_a) ],
    [ fill(detail(item_a), 1001, detail_row(item_a, body_a)) ],
    [ +select(item_b) ],
    [ fill(detail(item_b), 1002, detail_row(item_b, body_b)) ],
    [ unsubscribe(1) ] ]) :- ui_program(Prog).

% ═══════════════════════════════════════════════════════════════════════════
% CHECKS
% ═══════════════════════════════════════════════════════════════════════════

% ── 1. subscription rows and demand-gated laziness ──────────────────────────

check(subscribe_plants_sub_and_demand_rows,
  ( run_prefix(timer_demanded_then_torn_down, 1, Final, _),
    rel_rows(sub/3, Final, [ sub(1, 0, clock5) ]),
    rel_rows(sub_path/2, Final, [ sub_path(1, [1]) ]),
    rel_rows(demand/4, Final, [ demand(DemandId, 1, clock5, 7) ]),
    integer(DemandId) )).

check(sub_forest_keys_are_integers,
  ( run_prefix(switch_map_swaps_scopes, 4, Final, _),
    findall(SubId, member(sub(SubId, _, _), Final), SubIds),
    findall(ParentId, member(sub(_, ParentId, _), Final), ParentIds),
    findall(Segment, ( member(sub_path(_, Path), Final), member(Segment, Path) ), Segments),
    findall(DemandId, member(demand(DemandId, _, _, _), Final), DemandIds),
    findall(Salt, member(demand(_, _, _, Salt), Final), Salts),
    append([SubIds, ParentIds, Segments, DemandIds, Salts], Keys),
    all_integers(Keys) )).

check(demanded_view_is_derived_from_demand_rows,
  ( run_prefix(timer_demanded_then_torn_down, 1, Final, _),
    rel_rows(demanded/2, Final, [ demanded(clock5, 7) ]) )).

check(timer_silent_without_demand,
  ( run_named(timer_never_subscribed, Final, DeltaTicks),
    rel_rows(tick_row/2, Final, []),
    rel_deltas(tick_row/2, DeltaTicks, [ [], [], [] ]) )).

check(timer_fires_only_while_demanded,
  ( run_named(timer_demanded_then_torn_down, Final, DeltaTicks),
    rel_rows(tick_row/2, Final, [ tick_row(clock5, 1), tick_row(clock5, 2) ]),
    rel_deltas(tick_row/2, DeltaTicks,
               [ [ +tick_row(clock5, 1) ], [ +tick_row(clock5, 2) ], [], [] ]) )).

check(within_tick_order_decides_the_last_fill,
  ( run_named(timer_fill_before_unsubscribe, Final, _),
    rel_rows(tick_row/2, Final,
             [ tick_row(clock5, 1), tick_row(clock5, 2) ]) )).

check(subscribe_under_dead_scope_throws,
  ( catch(( run_named(subscribe_under_dead_scope, _, _), fail ), Thrown, true),
    Thrown == subscribe_under_dead_scope(1) )).

% ── 2. teardown: both halves ────────────────────────────────────────────────

check(teardown_deletes_the_subtree_rows,
  ( run_named(teardown_keeps_history, Final, _),
    forest_rows(Final, []) )).

check(teardown_retracts_the_scoped_level_view,
  ( run_named(teardown_keeps_history, Final, DeltaTicks),
    rel_rows(live_sample/1, Final, []),
    rel_deltas(live_sample/1, DeltaTicks,
               [ [ +live_sample(11) ], [ +live_sample(12) ],
                 [ -live_sample(11), -live_sample(12) ], [] ]) )).

check(teardown_leaves_log_history_alone,
  ( run_named(teardown_keeps_history, Final, _),
    rel_rows(sample_log/1, Final, [ sample_log(11), sample_log(12) ]),
    rel_rows(sample/2, Final, [ sample(sensor, 11), sample(sensor, 12) ]) )).

check(teardown_is_visible_as_forest_deltas,
  ( run_named(teardown_keeps_history, _, DeltaTicks),
    rel_deltas(demand/4, DeltaTicks, DemandDeltas),
    nth1(3, DemandDeltas, [ -demand(_, 1, sensor, 7) ]) )).

check(post_teardown_fill_writes_nothing,
  ( run_named(teardown_keeps_history, Final, _),
    \+ member(sample(sensor, 13), Final) )).

% ── 3. switch_map ───────────────────────────────────────────────────────────

check(switch_map_plants_an_inner_scope,
  ( run_prefix(switch_map_swaps_scopes, 2, Final, _),
    rel_rows(sub/3, Final, [ sub(1, 0, panel), sub(1001, 1, detail(item_a)) ]),
    rel_rows(sub_path/2, Final, [ sub_path(1, [1]), sub_path(1001, [1, 1001]) ]) )).

check(switch_map_salt_is_the_fresh_sub_id,
  ( run_prefix(switch_map_swaps_scopes, 2, Final, _),
    member(demand(_, 1001, detail(item_a), 1001), Final) )).

check(switch_map_tears_down_the_previous_inner,
  ( run_prefix(switch_map_swaps_scopes, 4, Final, _),
    \+ member(sub(1001, _, _), Final),
    \+ member(demand(_, 1001, _, _), Final),
    \+ member(sub_path(1001, _), Final),
    member(sub(1002, 1, detail(item_b)), Final),
    member(sub(1, 0, panel), Final) )).

check(switch_map_dead_scope_view_retracts,
  ( run_named(switch_map_swaps_scopes, Final, DeltaTicks),
    rel_deltas(detail_view/2, DeltaTicks,
               [ [], [], [ +detail_view(item_a, body_a) ],
                 [ -detail_view(item_a, body_a) ],
                 [ +detail_view(item_b, body_b) ] ]),
    rel_rows(detail_view/2, Final, [ detail_view(item_b, body_b) ]) )).

check(switch_map_dead_scope_data_survives,
  ( run_named(switch_map_swaps_scopes, Final, _),
    rel_rows(detail_row/2, Final,
             [ detail_row(item_a, body_a), detail_row(item_b, body_b) ]) )).

% the stale-response ruling, graded: an in-flight fill for a DEAD demand row
% is DROPPED, because the demand row IS the request and the response has no
% request to attach to.
check(stale_fill_for_dead_scope_is_dropped,
  ( run_named(switch_map_swaps_scopes, Final, _),
    \+ member(detail_row(item_a, late_body), Final) )).

% and the salt is what makes it stale: the target is demanded again, under a
% new scope, and the old fill is still refused.
check(stale_fill_refused_even_when_target_resubscribed,
  ( run_named(switch_map_resubscribed_target, Final, _),
    member(sub(1003, 1, detail(item_a)), Final),
    \+ member(detail_row(item_a, stale_body), Final),
    member(detail_row(item_a, fresh_body), Final) )).

% ── 4. dominance, operational ───────────────────────────────────────────────

check(dominated_timer_stops_at_outer_completion,
  ( run_named(dominated_timer, Final, DeltaTicks),
    rel_rows(poll/2, Final, [ poll(session_one, 1), poll(session_one, 2) ]),
    rel_deltas(poll/2, DeltaTicks,
               [ [], [], [ +poll(session_one, 1) ], [ +poll(session_one, 2) ],
                 [], [], [] ]) )).

check(outer_completion_empties_the_scope_subtree,
  ( run_named(dominated_timer, Final, _),
    forest_rows(Final, []) )).

check(undominated_timer_keeps_firing,
  ( run_named(undominated_timer, Final, _),
    rel_rows(poll/2, Final,
             [ poll(session_one, 1), poll(session_one, 2),
               poll(session_one, 3), poll(session_one, 4) ]),
    member(sub(2, 0, every300(session_one)), Final) )).

% the same run says WHY it keeps firing: demand is refcounted by IVM support.
% The dominated scope 1001 IS torn down at tick 5 in this run too; the root
% subscription's demand row still supports demanded(every300(...), 1001), so
% the request survives its second subscriber.
check(demand_is_refcounted_by_support,
  ( run_named(undominated_timer, Final, DeltaTicks),
    \+ member(sub(1001, _, _), Final),
    member(demanded(every300(session_one), 1001), Final),
    rel_deltas(demand/4, DeltaTicks, DemandDeltas),
    nth1(5, DemandDeltas, FifthTick),
    memberchk(-demand(_, 1001, every300(session_one), 1001), FifthTick),
    rel_deltas(demanded/2, DeltaTicks, DemandedDeltas),
    nth1(5, DemandedDeltas, [ -demanded(outer, 7) ]) )).

% the mode_lab claim, restated as a runtime difference: same canned timer,
% same fills, two lifetimes, and the only difference is where the sub row sits.
check(dominance_is_the_only_difference_between_the_two_runs,
  ( run_named(dominated_timer, DominatedFinal, _),
    run_named(undominated_timer, UndominatedFinal, _),
    rel_rows(poll/2, DominatedFinal, DominatedPolls),
    rel_rows(poll/2, UndominatedFinal, UndominatedPolls),
    length(DominatedPolls, 2), length(UndominatedPolls, 4),
    ord_subtract(UndominatedPolls, DominatedPolls,
                 [ poll(session_one, 3), poll(session_one, 4) ]) )).

% ── 5. forkJoin ─────────────────────────────────────────────────────────────

check(fork_join_is_silent_while_an_arm_is_outstanding,
  ( run_named(fork_join_self_teardown, _, DeltaTicks),
    rel_deltas(combined/2, DeltaTicks, CombinedDeltas),
    nth1(2, CombinedDeltas, []) )).

check(fork_join_scope_deletes_its_own_demand_rows,
  ( run_named(fork_join_self_teardown, Final, DeltaTicks),
    forest_rows(Final, []),
    rel_deltas(demand/4, DeltaTicks, DemandDeltas),
    nth1(3, DemandDeltas, ThirdTick),
    length(ThirdTick, 3),
    forall(member(Delta, ThirdTick), Delta = -_) )).

check(fork_join_combined_row_outlives_the_scope,
  ( run_named(fork_join_self_teardown, Final, _),
    rel_rows(combined/2, Final, [ combined(alpha, beta) ]) )).

check(fork_join_level_view_dies_with_the_scope,
  ( run_named(fork_join_self_teardown, Final, _),
    rel_rows(fork_view/2, Final, []) )).

check(fork_join_late_arm_fill_is_dropped,
  ( run_named(fork_join_self_teardown, Final, _),
    rel_rows(result_a/1, Final, [ result_a(alpha) ]) )).

% ── 6. repeat ───────────────────────────────────────────────────────────────

check(repeat_resubscribe_is_a_new_sub_row,
  ( run_named(repeat_fresh_salt, Final, _),
    rel_rows(sub/3, Final, [ sub(2, 0, job) ]),
    \+ member(sub(1, _, _), Final) )).

check(repeat_fresh_salt_refires_the_effect,
  ( run_named(repeat_fresh_salt, Final, _),
    rel_rows(fire_log/2, Final,
             [ fire_log(job, 101), fire_log(job, 102) ]),
    rel_rows(job_done/1, Final, [ job_done(1), job_done(2) ]) )).

check(repeat_iterations_are_distinct_occurrences,
  ( run_named(repeat_fresh_salt, _, DeltaTicks),
    rel_deltas(job_done/1, DeltaTicks, JobDeltas),
    nth1(2, JobDeltas, [ +job_done(1) ]),
    nth1(4, JobDeltas, [ +job_done(2) ]) )).

check(resubscribe_with_the_same_salt_is_silent,
  ( run_named(repeat_same_salt_is_silent, Final, DeltaTicks),
    rel_rows(fire_log/2, Final, [ fire_log(job, 101) ]),
    rel_deltas(demanded/2, DeltaTicks, DemandedDeltas),
    nth1(2, DemandedDeltas, []),
    rel_rows(demand/4, Final, TwoDemandRows),
    length(TwoDemandRows, 2) )).

% ── 7. the UI worked example ────────────────────────────────────────────────

check(ui_panel_subscribe_shows_the_filtered_view,
  ( run_prefix(ui_panel_session, 1, Final, DeltaTicks),
    rel_rows(visible_item/2, Final,
             [ visible_item(item_a, alpha), visible_item(item_b, beta) ]),
    rel_deltas(visible_item/2, DeltaTicks,
               [ [ +visible_item(item_a, alpha), +visible_item(item_b, beta) ] ]) )).

% ONLY DELTAS CROSS THE COASTLINE: the tick that adds one row sends one delta,
% not the three-row view.
check(ui_new_row_sends_one_delta_not_a_scan,
  ( run_prefix(ui_panel_session, 2, Final, DeltaTicks),
    rel_deltas(visible_item/2, DeltaTicks, [ _, SecondTick ]),
    SecondTick == [ +visible_item(item_c, gamma) ],
    rel_rows(visible_item/2, Final, ThreeRows),
    length(ThreeRows, 3) )).

check(ui_detail_pane_swaps_on_reselect,
  ( run_prefix(ui_panel_session, 6, Final, DeltaTicks),
    rel_deltas(detail_view/2, DeltaTicks,
               [ [], [], [], [ +detail_view(item_a, body_a) ],
                 [ -detail_view(item_a, body_a) ],
                 [ +detail_view(item_b, body_b) ] ]),
    rel_rows(detail_view/2, Final, [ detail_view(item_b, body_b) ]),
    rel_rows(sub/3, Final, [ sub(1, 0, item_list), sub(1002, 1, detail(item_b)) ]) )).

check(ui_close_empties_the_forest,
  ( run_named(ui_panel_session, Final, _),
    forest_rows(Final, []),
    rel_rows(visible_item/2, Final, []),
    rel_rows(detail_view/2, Final, []) )).

check(ui_session_data_survives_the_close,
  ( run_named(ui_panel_session, Final, _),
    rel_rows(ui_log/2, Final, [ ui_log(selected, item_a), ui_log(selected, item_b) ]),
    rel_rows(detail_row/2, Final,
             [ detail_row(item_a, body_a), detail_row(item_b, body_b) ]),
    rel_rows(item/3, Final,
             [ item(item_a, alpha, active), item(item_b, beta, active),
               item(item_c, gamma, active), item(item_z, zeta, archived) ]),
    rel_rows(select/1, Final, [ select(item_a), select(item_b) ]) )).

% ruling r4 meets teardown: level views dying under a scope DO fire departed
% rules, and the consequence lands in an unscoped Log rel, so it survives.
check(ui_teardown_fires_departure_rules,
  ( run_named(ui_panel_session, Final, DeltaTicks),
    rel_rows(closed/1, Final, [ closed(item_a), closed(item_b), closed(item_c) ]),
    rel_deltas(closed/1, DeltaTicks, ClosedDeltas),
    nth1(7, ClosedDeltas, []),
    nth1(8, ClosedDeltas, EighthTick),
    length(EighthTick, 3) )).

% the delta stream IS the view state: folding what the UI received reproduces
% the rowset it would have got from a table scan, at every tick.
check(ui_delta_fold_reconstructs_the_view,
  ( forall(member(Length, [1, 2, 3, 4, 5, 6]),
           ( run_prefix(ui_panel_session, Length, Final, DeltaTicks),
             rel_deltas(visible_item/2, DeltaTicks, VisibleDeltas),
             fold_delta_stream(VisibleDeltas, VisibleFolded),
             rel_rows(visible_item/2, Final, VisibleFolded),
             rel_deltas(detail_view/2, DeltaTicks, DetailDeltas),
             fold_delta_stream(DetailDeltas, DetailFolded),
             rel_rows(detail_view/2, Final, DetailFolded) )) )).

% The whole session moved 10 deltas across the coastline: 3 list rows in, 3
% out at close, 2 detail rows in, 2 out on swap and close. A per-tick scan of
% the same two views over the same 9 ticks could not have gone below 9 * 3.
check(ui_whole_session_is_bounded_delta_traffic,
  ( run_named(ui_panel_session, _, DeltaTicks),
    length(DeltaTicks, 9),
    rel_deltas(visible_item/2, DeltaTicks, VisibleDeltas),
    rel_deltas(detail_view/2, DeltaTicks, DetailDeltas),
    append(VisibleDeltas, DetailDeltas, AllDeltas),
    append(AllDeltas, Flat),
    length(Flat, Total),
    Total == 10,
    ScanFloor is 9 * 3,
    Total < ScanFloor )).

go :- run(check).

% ═══════════════════════════════════════════════════════════════════════════
% trace printer (receipts for the .md)
% ═══════════════════════════════════════════════════════════════════════════

report :-
    forall(scenario(Name, _, _, _),
           ( format("~w~n", [Name]),
             (   catch(run_named(Name, Final, DeltaTicks), Thrown, true)
             ->  (   var(Thrown)
                 ->  forall(nth1(Index, DeltaTicks, Deltas),
                            format("  tick ~w  ~q~n", [Index, Deltas])),
                     forest_rows(Final, Forest),
                     format("  forest   ~q~n", [Forest]),
                     format("  final    ~q~n", [Final])
                 ;   format("  REJECTED ~q~n", [Thrown]) )
             ;   format("  no solution~n", []) ),
             nl )).
