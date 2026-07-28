% switch_flow.pl : can switchMap ITSELF flow, and what is complete?
%
% Run:    swipl -q -l v6/prolog/labs/switch_flow.pl -g go -g halt
% Trace:  swipl -q -l v6/prolog/labs/switch_flow.pl -g report -g halt
%
% Contract: plans/2026-07-27-switch-flow-lab-header.md (Q1..Q5).
% Base: plans/2026-07-27-sub-forest.md (the forest, recoverable at
% `git show 2fff3f61:v6/prolog/labs/sub_lifetimes.pl`) and
% plans/2026-07-27-mode-lattice.md (the two operators, recoverable at
% `git show 2fff3f61:v6/prolog/labs/mode_lab.pl`). Both are reused verbatim
% here, not reinvented: the forest rows and teardown are sub_lifetimes',
% scope_min/join_max/dnf_lifetime are mode_lab's.
%
% ── WHAT THIS LAB CHANGES IN THE FOREST ─────────────────────────────────────
%
% ONE modification and ONE column, both forced by graded checks:
%
%   A. The switch fires on the OCCURRENCE STREAM, not on the outside-arrival
%      list. sub_lifetimes matched switch_scope patterns inside phase 0
%      (apply_items), so a switch keyed by a state register could never fire
%      at all: a register is written by an edge rule and is never an outside
%      arrival. Moving the match to the same alphabet edge rules already see
%      (carry-in, then arrivals, then newly-true level rows) makes the
%      register-keyed switch work AND makes a same-tick state flap net to zero
%      scope churn for free, because the carry is already filtered to
%      boundary-observable writes (R2 rider + q4 next_tick).
%
%   B. switch_scope grows a fourth column, the flattening Policy:
%      switch | exhaust | merge | concat. Three of the four need no state at
%      all. concat needs an ordered pending set; the lab implements it as one
%      engine rel and ALSO reproduces it in userland with four ordinary rules,
%      so the kernel queue is a latency purchase, not a necessity.
%
% Forest rows (ruling storage_integer_keys: integer ids only):
%
%   sub(SubId, ParentId, Target)                   the script node, root 0
%   sub_path(SubId, [Segment, ...])                materialized path
%   demand(DemandId, SubId, Target, Salt)          the request rows
%   scope_queue(QueueId, ParentId, ParentPath, Target)   concat only
%
% and the one engine-injected level rule:
%
%   demanded(Target, Salt) <- demand(_, _, Target, Salt).

:- use_module('../src/grader.pl').
:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(library(ordsets)).

:- op(1150, xfx, <-).       % LEVEL rule: maintained view, consequences retract
:- op(1150, xfx, <+).       % EDGE rule:  fires on occurrences, appends
:- op(700,  xfx, :=).

:- discontiguous scenario/4, check/2.

drain_cap(100).
first_allocated_id(1000).   % hand-written scenarios use small ids; the engine
                            % counters start above them so the two never clash

% ═══════════════════════════════════════════════════════════════════════════
% 1. THE LIFETIME LATTICE (mode_lab, reused verbatim)
% ═══════════════════════════════════════════════════════════════════════════
% finite = TRUE, never = FALSE, until(Clauses) = a monotone DNF formula over
% end-signal names. scope_min is OR (dominance), join_max is AND (a rule body).

until_signal(Signal, until([[Signal]])).

dnf_lifetime(Clauses, Lifetime) :-
    (   Clauses == []          -> Lifetime = never
    ;   memberchk([], Clauses) -> Lifetime = finite
    ;   Lifetime = until(Clauses)
    ).

normalize_clauses(RawClauses, Normalized) :-
    maplist(sort, RawClauses, PerClauseSorted),
    sort(PerClauseSorted, UniqueClauses),
    exclude(strictly_absorbed(UniqueClauses), UniqueClauses, Normalized).

strictly_absorbed(AllClauses, Clause) :-
    member(OtherClause, AllClauses),
    OtherClause \== Clause,
    ord_subset(OtherClause, Clause).

scope_min(Left, Right, Result) :-
    (   Left == finite  -> Result = finite
    ;   Right == finite -> Result = finite
    ;   Left == never   -> Result = Right
    ;   Right == never  -> Result = Left
    ;   Left = until(LeftClauses), Right = until(RightClauses),
        append(LeftClauses, RightClauses, Combined),
        normalize_clauses(Combined, Normalized),
        dnf_lifetime(Normalized, Result)
    ).

join_max(Left, Right, Result) :-
    (   Left == never   -> Result = never
    ;   Right == never  -> Result = never
    ;   Left == finite  -> Result = Right
    ;   Right == finite -> Result = Left
    ;   Left = until(LeftClauses), Right = until(RightClauses),
        findall(Product,
                ( member(LeftClause, LeftClauses),
                  member(RightClause, RightClauses),
                  append(LeftClause, RightClause, Product) ),
                Products),
        normalize_clauses(Products, Normalized),
        dnf_lifetime(Normalized, Result)
    ).

% THE COMPLETION CALCULUS AT RUNTIME. Given when each end-signal actually
% fired, the formula names the tick the subscription must die on: a clause is
% satisfied when its LAST signal has fired (max), and the formula is satisfied
% by its EARLIEST satisfied clause (min). Section 3's checks compare this
% number against the tick the forest actually lost the sub row.
formula_first_true(finite, _, 0).
formula_first_true(never, _, none).
formula_first_true(until(Clauses), SignalTicks, Answer) :-
    findall(ClauseTick,
            ( member(Clause, Clauses),
              clause_first_true(Clause, SignalTicks, ClauseTick),
              integer(ClauseTick) ),
            SatisfiedTicks),
    (   SatisfiedTicks == []
    ->  Answer = none
    ;   min_list(SatisfiedTicks, Answer) ).

clause_first_true(Signals, SignalTicks, Answer) :-
    (   maplist(signal_fire_tick(SignalTicks), Signals, PerSignal)
    ->  max_list(PerSignal, Answer)
    ;   Answer = none ).

signal_fire_tick(SignalTicks, Signal, Tick) :- memberchk(Signal-Tick, SignalTicks).

% ═══════════════════════════════════════════════════════════════════════════
% 2. THE STORE
% ═══════════════════════════════════════════════════════════════════════════
% srow(Row) for Set rels; lrow(st(Tick, Seq), Row) for Log rels. Level views
% are computed. The id counters are engine state, NOT store rows.

rel_ref(Atom, Name/Arity) :- functor(Atom, Name, Arity).

rel_kind(Decls, Ref, log) :- memberchk(kind(Ref, log), Decls), !.
rel_kind(Decls, Ref, set) :- memberchk(keyed(Ref, _), Decls), !.
rel_kind(_, _, set).

decl_key(Decls, Ref, Positions) :- memberchk(keyed(Ref, Positions), Decls).

key_of(Positions, Row, Key) :-
    Row =.. [_ | Arguments],
    findall(Column, ( member(Position, Positions), nth1(Position, Arguments, Column) ), Key).

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
    forall(( member((_ <- Body), Rules), body_departed_ref(Body, DepartedRef) ),
           throw(departed_in_level_rule(DepartedRef))).

% ═══════════════════════════════════════════════════════════════════════════
% 3. BODY SOLVING
% ═══════════════════════════════════════════════════════════════════════════
% ctx(Visible, PreState, Tick). pre/1 reads the evolving pre-state (r6).

solve(true, _) :- !.
solve((Left, Right), Ctx) :- !, solve(Left, Ctx), solve(Right, Ctx).
solve(not(Goal), Ctx) :- !, \+ solve(Goal, Ctx).
solve(only(Goal), Ctx) :- !, solve(Goal, Ctx).
solve(departed(_), _) :- !, fail.          % never satisfiable as a read
solve(pre(Atom), ctx(_, PreState, _)) :- !, member(Atom, PreState).
solve(now(Tick), ctx(_, _, Tick)) :- !.
solve((Variable := Expression), _) :- !, eval_expr(Expression, Value), Variable = Value.
solve(Goal, _) :- comparison_goal(Goal), !, call(Goal).
solve(Atom, ctx(Visible, _, _)) :- member(Atom, Visible).

eval_expr(Expression, Value) :- arithmetic_expr(Expression), !, Value is Expression.
eval_expr(Expression, Expression).

arithmetic_expr(Expression) :- integer(Expression), !.
arithmetic_expr(Expression) :-
    compound(Expression), functor(Expression, Name, 2),
    memberchk(Name, [+, -, *, //, mod]), ground(Expression).

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
% 4. LEVEL CLOSURE
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
% 5. THE FOREST: subscribe, teardown, demand lookup, the concat queue
% ═══════════════════════════════════════════════════════════════════════════

sub_path_of(0, _, []) :- !.
sub_path_of(SubId, Store, Path) :- memberchk(srow(sub_path(SubId, Path)), Store).

forest_subscribe(SubId, ParentId, Target, Salt, Store0,
                 ids(SubSeq, DemandSeq0, QueueSeq), Store,
                 ids(SubSeq, DemandSeq, QueueSeq)) :-
    (   sub_path_of(ParentId, Store0, ParentPath)
    ->  true
    ;   throw(subscribe_under_dead_scope(ParentId)) ),
    append(ParentPath, [SubId], Path),
    DemandSeq is DemandSeq0 + 1,
    Store = [ srow(sub(SubId, ParentId, Target)),
              srow(sub_path(SubId, Path)),
              srow(demand(DemandSeq, SubId, Target, Salt)) | Store0 ].

% Teardown = range-DELETE by path prefix, now over FOUR tables (sub, sub_path,
% demand, scope_queue). Mode inclusive kills the scope with its children;
% children leaves the scope standing (switch's next value).
forest_teardown(SubId, Mode, Store0, Store) :-
    (   sub_path_of(SubId, Store0, Path)
    ->  findall(DeadId,
                ( member(srow(sub_path(DeadId, OtherPath)), Store0),
                  path_covered(Mode, Path, OtherPath) ),
                DeadIds),
        exclude(dead_forest_row(DeadIds, Mode, Path), Store0, Store)
    ;   Store = Store0 ).

path_covered(Mode, Path, OtherPath) :-
    append(Path, Suffix, OtherPath),
    ( Mode == inclusive -> true ; Suffix \== [] ).

dead_forest_row(DeadIds, _, _, srow(sub(SubId, _, _)))       :- memberchk(SubId, DeadIds).
dead_forest_row(DeadIds, _, _, srow(sub_path(SubId, _)))     :- memberchk(SubId, DeadIds).
dead_forest_row(DeadIds, _, _, srow(demand(_, SubId, _, _))) :- memberchk(SubId, DeadIds).
dead_forest_row(_, Mode, Path, srow(scope_queue(_, _, QueuePath, _))) :-
    path_covered(Mode, Path, QueuePath).

% The laziness gate. It consults the demand SET, however that set is produced:
% stored demand/4 rows in the forest model, or an ordinary DERIVED demanded/2
% relation in the minimal-kernel model of section 15. One gate, both worlds.
demand_present(_, Store, Target, Salt) :-
    member(srow(demand(_, _, Target, Salt)), Store), !.
demand_present(prog(_, Rules), Store, Target, Salt) :-
    store_rows(Store, Rows),
    level_closure(Rules, Rows, 0, Level),
    memberchk(demanded(Target, Salt), Level), !.

has_live_child(Store, ParentId) :- member(srow(sub(_, ParentId, _)), Store), !.

% A swap plants under a scope that may already be dead (its panel closed on an
% earlier item of the same tick). That is silence, not an error; the explicit
% subscribe item keeps the throw.
plant_child(ParentId, Target, Store0, ids(SubSeq0, DemandSeq0, QueueSeq),
            Store, Ids) :-
    (   sub_path_of(ParentId, Store0, _)
    ->  NewSubId is SubSeq0 + 1,
        forest_subscribe(NewSubId, ParentId, Target, NewSubId, Store0,
                         ids(NewSubId, DemandSeq0, QueueSeq), Store, Ids)
    ;   Store = Store0, Ids = ids(SubSeq0, DemandSeq0, QueueSeq) ).

enqueue_scope(ParentId, Target, Store0, ids(SubSeq, DemandSeq, QueueSeq0),
              Store, ids(SubSeq, DemandSeq, QueueSeq)) :-
    (   sub_path_of(ParentId, Store0, ParentPath)
    ->  QueueSeq is QueueSeq0 + 1,
        Store = [ srow(scope_queue(QueueSeq, ParentId, ParentPath, Target)) | Store0 ]
    ;   Store = Store0, QueueSeq = QueueSeq0 ).

% The concat drain: the lowest queue id whose parent is alive and childless.
% Dense integer ids ARE the arrival order, so "oldest first" is one msort.
next_queued_scope(Store, QueueId, ParentId, ParentPath, Target) :-
    findall(Id-queued(Parent, Path, Wanted),
            ( member(srow(scope_queue(Id, Parent, Path, Wanted)), Store),
              memberchk(srow(sub(Parent, _, _)), Store),
              \+ has_live_child(Store, Parent) ),
            Candidates),
    Candidates \== [],
    msort(Candidates, [QueueId-queued(ParentId, ParentPath, Target) | _]).

% ═══════════════════════════════════════════════════════════════════════════
% 6. PHASE 0: arrivals, explicit forest ops, and world fills, IN ORDER
% ═══════════════════════════════════════════════════════════════════════════
% +Row / -Row                        outside arrival
% subscribe(SubId, ParentId, Target, Salt)   plant a scope explicitly
% unsubscribe(SubId) / complete(SubId)       tear the scope down (inclusive)
% fill(Target, Salt, Row)            a bind fill, demand-gated
%
% CHANGE A: scope swaps are NOT applied here any more. They ride the
% occurrence stream in section 7.

apply_items(_, _, [], Store, Ids, Seq, Store, Ids, Seq, []).
apply_items(Prog, Tick, [Item | Rest], Store0, Ids0, Seq0,
            Store, Ids, Seq, Occurrences) :-
    apply_item(Prog, Tick, Item, Store0, Ids0, Seq0, Store1, Ids1, Seq1, Here),
    apply_items(Prog, Tick, Rest, Store1, Ids1, Seq1, Store, Ids, Seq, Later),
    append(Here, Later, Occurrences).

apply_item(Prog, Tick, +Row, Store0, Ids, Seq0, Store, Ids, Seq, Occurrences) :- !,
    absorb_row(Prog, Tick, Row, Store0, Seq0, Store, Seq, Occurrences).
apply_item(prog(Decls, _), _, -Row, Store0, Ids, Seq, Store, Ids, Seq, []) :- !,
    rel_ref(Row, Ref),
    ( rel_kind(Decls, Ref, log) -> throw(retract_from_log(Ref)) ; true ),
    exclude(==(srow(Row)), Store0, Store).
apply_item(_, _, subscribe(SubId, ParentId, Target, Salt), Store0, Ids0, Seq,
           Store, Ids, Seq, []) :- !,
    forest_subscribe(SubId, ParentId, Target, Salt, Store0, Ids0, Store, Ids).
apply_item(_, _, unsubscribe(SubId), Store0, Ids, Seq, Store, Ids, Seq, []) :- !,
    forest_teardown(SubId, inclusive, Store0, Store).
apply_item(_, _, complete(SubId), Store0, Ids, Seq, Store, Ids, Seq, []) :- !,
    forest_teardown(SubId, inclusive, Store0, Store).
apply_item(Prog, Tick, fill(Target, Salt, Row), Store0, Ids, Seq0,
           Store, Ids, Seq, Occurrences) :- !,
    (   demand_present(Prog, Store0, Target, Salt)
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

% ═══════════════════════════════════════════════════════════════════════════
% 7. THE SWITCH, AS A RULE OVER THE OCCURRENCE STREAM
% ═══════════════════════════════════════════════════════════════════════════
% switch_scope(Pattern, ParentScope, Target, Policy). Every argument may be a
% shared variable, so the whole switch can come out of one row (Q1).
%
% NEVER findall over the decls: findall copies its template and would sever
% Pattern from Target, so `select(ItemId)` would stop binding `detail(ItemId)`.
% foldl over the decl list with an explicit copy_term per decl keeps the
% sharing, which is the same law unmarked_items/2 obeys in the engine.

apply_scope_swaps(prog(Decls, _), Row, Store0, Ids0, Store, Ids) :-
    foldl(swap_for_decl(Row), Decls, Store0-Ids0, Store-Ids).

swap_for_decl(Row, Decl, Store0-Ids0, Store-Ids) :-
    (   Decl = switch_scope(Pattern0, ParentScope0, Target0, Policy),
        copy_term(switch(Pattern0, ParentScope0, Target0),
                  switch(Pattern, ParentScope, Target)),
        Pattern = Row
    ->  apply_one_swap(Policy, ParentScope, Target, Store0, Ids0, Store, Ids)
    ;   Store = Store0, Ids = Ids0 ).

% switchMap: the new value replaces the old scope.
apply_one_swap(switch, ParentId, Target, Store0, Ids0, Store, Ids) :-
    forest_teardown(ParentId, children, Store0, Store1),
    plant_child(ParentId, Target, Store1, Ids0, Store, Ids).
% mergeMap: every value gets its own scope, all siblings.
apply_one_swap(merge, ParentId, Target, Store0, Ids0, Store, Ids) :-
    plant_child(ParentId, Target, Store0, Ids0, Store, Ids).
% exhaustMap: a value arriving while a scope lives is DISCARDED.
apply_one_swap(exhaust, ParentId, Target, Store0, Ids0, Store, Ids) :-
    (   has_live_child(Store0, ParentId)
    ->  Store = Store0, Ids = Ids0
    ;   plant_child(ParentId, Target, Store0, Ids0, Store, Ids) ).
% concatMap: a value arriving while a scope lives is QUEUED in arrival order.
apply_one_swap(concat, ParentId, Target, Store0, Ids0, Store, Ids) :-
    (   has_live_child(Store0, ParentId)
    ->  enqueue_scope(ParentId, Target, Store0, Ids0, Store, Ids)
    ;   plant_child(ParentId, Target, Store0, Ids0, Store, Ids) ).

% ═══════════════════════════════════════════════════════════════════════════
% 8. EDGE FIRING, ONE OCCURRENCE AT A TIME (swaps ride along)
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

process_occurrences(_, _, _, [], Store, Ids, Store, Ids, []).
process_occurrences(Prog, Tick, frozen(MidLevel, PrevLevel), [occ(_, Payload) | Rest],
                    Store0, Ids0, Store, Ids, Written) :-
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
    swaps_for_payload(Prog, Payload, Store1, Ids0, Store2, Ids1),
    process_occurrences(Prog, Tick, frozen(MidLevel, PrevLevel), Rest,
                        Store2, Ids1, Store, Ids, WrittenRest),
    append(WrittenHere, WrittenRest, Written).

% A departure is a row LEAVING; the switch fires on values arriving only.
swaps_for_payload(_, dep(_), Store, Ids, Store, Ids) :- !.
swaps_for_payload(Prog, Row, Store0, Ids0, Store, Ids) :-
    apply_scope_swaps(Prog, Row, Store0, Ids0, Store, Ids).

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
% 9. RETENTION (q10)
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
% 10. THE SETTLING PHASE: completion teardown, then the concat drain
% ═══════════════════════════════════════════════════════════════════════════
% A scope_done(SubId) level row means the scope has all its terminal rows.
% The phase loops because one completion frees an enclosing scope's condition
% AND because a completion can free a queued scope, which can complete too.

settle_scopes(Prog, Tick, Store0, Ids0, Store, Ids, Level) :-
    Prog = prog(_, Rules),
    store_rows(Store0, Rows),
    level_closure(Rules, Rows, Tick, Level0),
    findall(SubId, ( member(scope_done(SubId), Level0),
                     memberchk(srow(sub(SubId, _, _)), Store0) ), Finished0),
    sort(Finished0, Finished),
    (   Finished \== []
    ->  foldl(teardown_inclusive, Finished, Store0, Store1),
        settle_scopes(Prog, Tick, Store1, Ids0, Store, Ids, Level)
    ;   next_queued_scope(Store0, QueueId, ParentId, ParentPath, Target)
    ->  exclude(==(srow(scope_queue(QueueId, ParentId, ParentPath, Target))),
                Store0, Store1),
        plant_child(ParentId, Target, Store1, Ids0, Store2, Ids1),
        settle_scopes(Prog, Tick, Store2, Ids1, Store, Ids, Level)
    ;   Store = Store0, Ids = Ids0, Level = Level0 ).

teardown_inclusive(SubId, Store0, Store) :-
    forest_teardown(SubId, inclusive, Store0, Store).

% ═══════════════════════════════════════════════════════════════════════════
% 11. THE TICK
% ═══════════════════════════════════════════════════════════════════════════
% state(Tick, Store, ids(SubSeq, DemandSeq, QueueSeq), PrevLevel, PrevAll)

tick(Prog, state(Tick, Store0, Ids0, PrevLevel, PrevAll), CarryIn, Items,
     state(NextTick, Store, Ids, Level, NextAll), CarryOut, Deltas) :-
    Prog = prog(_, Rules),
    apply_items(Prog, Tick, Items, Store0, Ids0, 1, StoreArrived, Ids1, _, ArrivalOccs),
    store_rows(StoreArrived, MidBase),
    level_closure(Rules, MidBase, Tick, MidLevel),
    ord_subtract(MidLevel, PrevLevel, NewLevelRows),
    stamp_extra(Tick, NewLevelRows, 1000, LevelOccs),
    stamp_extra(Tick, CarryIn, 2000, CarryOccs),
    append([CarryOccs, ArrivalOccs, LevelOccs], Occurrences),
    process_occurrences(Prog, Tick, frozen(MidLevel, PrevLevel), Occurrences,
                        StoreArrived, Ids1, StoreWritten, Ids2, WrittenRows),
    apply_retention(Prog, StoreWritten, StorePruned),
    settle_scopes(Prog, Tick, StorePruned, Ids2, Store, Ids, Level),
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

delta_ref_is_set(Decls, Row) :- rel_ref(Row, Ref), rel_kind(Decls, Ref, Kind), Kind == set.

dedupe_keep_order([], []).
dedupe_keep_order([Item | Rest], [Item | Out]) :-
    exclude(==(Item), Rest, Filtered), dedupe_keep_order(Filtered, Out).

% ═══════════════════════════════════════════════════════════════════════════
% 12. THE RUN LOOP
% ═══════════════════════════════════════════════════════════════════════════

% The engine injects demanded/2 over its own demand rows ONLY when the program
% does not head demanded/2 itself. A minimal-kernel program derives its demand
% set from its own rels (section 15), and then the engine contributes no rule.
run_program(prog(Decls, Rules0), Initial, Schedule, FinalAll, DeltaTicks) :-
    check_program(prog(Decls, Rules0)),
    (   member((demanded(_, _) <- _), Rules0)
    ->  Rules = Rules0
    ;   Rules = [ (demanded(Target, Salt) <- demand(_DemandId, _SubId, Target, Salt)) | Rules0 ] ),
    Prog = prog(Decls, Rules),
    seed_store(Prog, Initial, Store0),
    store_rows(Store0, BaseRows),
    level_closure(Rules, BaseRows, 0, Level0),
    append(BaseRows, Level0, All0), sort(All0, PrevAll),
    first_allocated_id(Start),
    run_ticks(Prog, state(1, Store0, ids(Start, Start, Start), Level0, PrevAll), [],
              Schedule, 0, FinalAll, DeltaTicks).

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
% 13. READING THE RESULTS
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

forest_rows(FinalAll, Forest) :-
    findall(Row,
            ( member(Row, FinalAll), rel_ref(Row, Ref),
              memberchk(Ref, [sub/3, sub_path/2, demand/4, scope_queue/4]) ),
            Forest0),
    msort(Forest0, Forest).

scope_targets(FinalAll, Targets) :-
    findall(Target, member(sub(_, _, Target), FinalAll), Targets0),
    msort(Targets0, Targets).

% the tick a sub row left the forest; `none` if it is still alive
scope_death_tick(DeltaTicks, SubId, Answer) :-
    (   nth1(Index, DeltaTicks, Deltas), memberchk(-sub(SubId, _, _), Deltas)
    ->  Answer = Index
    ;   Answer = none ).

scope_birth_ticks(DeltaTicks, Target, Ticks) :-
    findall(Index,
            ( nth1(Index, DeltaTicks, Deltas), member(+sub(_, _, Target), Deltas) ),
            Ticks).

all_integers(Terms) :- forall(member(Term, Terms), integer(Term)).

schedule_items(Name, Items) :-
    scenario(Name, _, _, Schedule), append(Schedule, Items).

% ═══════════════════════════════════════════════════════════════════════════
% SCENARIOS -- Q1: can the switch itself flow?
% ═══════════════════════════════════════════════════════════════════════════

% ── Q1a. switch keyed by a STATE REGISTER ───────────────────────────────────
% current_route is a keyed Set rel written only by an edge rule. It is never
% an outside arrival, so a switch that matched the arrival list could not fire
% at all. Riding the occurrence stream, the register write reaches the switch
% through the carry, one tick later (q4 next_tick).

register_program(
  prog([ kind(route_change/2, log), keep(route_change/2, all),
         keyed(current_route/2, [1]),
         kind(route_row/2, set),
         switch_scope(current_route(SessionId, RouteId), 1,
                      route_data(SessionId, RouteId), switch) ],
       [ (current_route(SessionId, RouteId) <+ only(route_change(SessionId, RouteId))),
         (route_view(RouteId, Body) <- demanded(route_data(_, RouteId), _),
                                       route_row(RouteId, Body)) ])).

scenario(register_drives_the_switch, Prog, [ current_route(session_one, home) ],
  [ [ subscribe(1, 0, shell_scope, 7) ],
    [ +route_change(session_one, settings) ],
    [ ],
    [ +route_change(session_one, profile) ],
    [ ] ]) :- register_program(Prog).

% ── Q1b. switch keyed by an ENUM ARM ────────────────────────────────────────
% The pattern is a nested envelope constructor: only the fresh arm plants a
% scope, and the other two arms cause zero scope churn.

enum_program(
  prog([ kind(fetch_result/2, log), keep(fetch_result/2, all),
         kind(body_row/2, set),
         switch_scope(fetch_result(Endpoint, fresh(Tag, _Body)), 1,
                      body_of(Endpoint, Tag), switch) ],
       [ (body_view(Tag, Field) <- demanded(body_of(_, Tag), _), body_row(Tag, Field)) ])).

scenario(enum_arm_drives_the_switch, Prog, [],
  [ [ subscribe(1, 0, feed, 7) ],
    [ +fetch_result(gh_repos, error(500)) ],
    [ +fetch_result(gh_repos, fresh(tag_v1, body_one)) ],
    [ +fetch_result(gh_repos, unchanged) ],
    [ +fetch_result(gh_repos, fresh(tag_v2, body_two)) ] ]) :- enum_program(Prog).

% ── Q1c. the TARGET comes out of a ROW ──────────────────────────────────────
% A routing table decides which target term a session subscribes to. The three
% targets have three different functors and arities, and there is exactly ONE
% switch_scope decl: rows carry ground terms, and a target IS a ground term.

routing_program(
  prog([ kind(open_session/2, log),   keep(open_session/2, all),
         kind(session_target/2, log), keep(session_target/2, all),
         kind(routing/2, set),
         switch_scope(session_target(_SessionId, TargetTerm), 1, TargetTerm, merge) ],
       [ (session_target(SessionId, TargetTerm) <+ only(open_session(SessionId, RouteId)),
                                                   routing(RouteId, TargetTerm)) ])).

scenario(routing_table_is_data, Prog,
  [ routing(fast, feed(fast_lane)),
    routing(slow, feed(slow_lane, wide_window)),
    routing(detail, detail_pane(item_a)) ],
  [ [ subscribe(1, 0, panel, 7) ],
    [ +open_session(session_one, fast) ],
    [ +open_session(session_two, detail) ],
    [ +open_session(session_three, slow) ],
    [ ] ]) :- routing_program(Prog).

% ── Q1d. the UNIVERSAL switch decl ──────────────────────────────────────────
% Pattern, parent scope and target are all shared variables over one row, so
% the decl carries no program-specific text at all. Everything a program wants
% to say about switching is then an ordinary rule feeding switch_to/2.

universal_program(
  prog([ kind(switch_to/2, log), keep(switch_to/2, all),
         switch_scope(switch_to(ParentScope, TargetTerm), ParentScope,
                      TargetTerm, switch) ],
       [])).

scenario(universal_switch_decl, Prog, [],
  [ [ subscribe(1, 0, root_scope, 7), subscribe(2, 0, other_scope, 8) ],
    [ +switch_to(1, alpha_target) ],
    [ +switch_to(2, beta_target) ],
    [ +switch_to(1, gamma_target) ] ]) :- universal_program(Prog).

% two switches in ONE tick under two different parents: the receipt that the
% decl's variables are copied per firing and never severed.
scenario(two_switches_one_tick, Prog, [],
  [ [ subscribe(1, 0, root_scope, 7), subscribe(2, 0, other_scope, 8) ],
    [ +switch_to(1, alpha_target), +switch_to(2, beta_target) ] ])
  :- universal_program(Prog).

% a switch naming a parent that closed earlier in the same tick is silence.
scenario(switch_under_a_closed_parent, Prog, [],
  [ [ subscribe(1, 0, root_scope, 7) ],
    [ unsubscribe(1), +switch_to(1, alpha_target) ] ]) :- universal_program(Prog).

% ═══════════════════════════════════════════════════════════════════════════
% SCENARIOS -- Q2: what is complete?
% ═══════════════════════════════════════════════════════════════════════════
% Three sources of scope_done, all of them ordinary level rules:
%   terminal enum arm   scope_done(Sub) <- sub(Sub,_,Target), stream_row(Target, done)
%   conjunctive body    scope_done(Sub) <- sub(Sub,_,_), result_a(_), result_b(_)
%   explicit rule head  scope_done(Sub) <- sub(Sub,_,_), close_request(_)

completion_program(Source,
  prog([ kind(stream_row/2, log),    keep(stream_row/2, all),
         kind(result_a/1, log),      keep(result_a/1, all),
         kind(result_b/1, log),      keep(result_b/1, all),
         kind(close_request/1, log), keep(close_request/1, all) ],
       [ (live_row(Target, Value) <- demanded(Target, _), stream_row(Target, Value))
       | CompletionRules ])) :- completion_rules(Source, CompletionRules).

completion_rules(terminal_arm,
  [ (scope_done(SubId) <- sub(SubId, _, Target), stream_row(Target, done)) ]).
completion_rules(conjunctive_body,
  [ (scope_done(SubId) <- sub(SubId, _, _), result_a(_), result_b(_)) ]).
completion_rules(explicit_head,
  [ (scope_done(SubId) <- sub(SubId, _, _), close_request(_)) ]).

scenario(completion_from(Source), Prog, [], Schedule) :-
    member(Source, [terminal_arm, conjunctive_body, explicit_head]),
    completion_program(Source, Prog),
    completion_schedule(Source, Schedule).

completion_schedule(terminal_arm,
  [ [ subscribe(1, 0, feed_one, 7) ],
    [ fill(feed_one, 7, stream_row(feed_one, value_one)) ],
    [ fill(feed_one, 7, stream_row(feed_one, done)) ],
    [ ] ]).
completion_schedule(conjunctive_body,
  [ [ subscribe(1, 0, feed_one, 7) ],
    [ fill(feed_one, 7, stream_row(feed_one, value_one)), +result_a(alpha) ],
    [ +result_b(beta) ],
    [ ] ]).
completion_schedule(explicit_head,
  [ [ subscribe(1, 0, feed_one, 7) ],
    [ fill(feed_one, 7, stream_row(feed_one, value_one)) ],
    [ +close_request(panel) ],
    [ ] ]).

% ── Q2b. the lattice as the runtime completion calculus ─────────────────────
% Outer scope 1 completes when BOTH end signals have fired: join_max.
% Inner scope 2 is under it and has its own end signal: scope_min.
% The formula's first-true tick must equal the tick the forest lost the row.

pipeline_program(
  prog([ kind(end_signal/1, log), keep(end_signal/1, all),
         kind(stage_row/2, log),  keep(stage_row/2, all) ],
       [ (stage_view(Target, Value) <- demanded(Target, _), stage_row(Target, Value)),
         (scope_done(1) <- end_signal(end_a), end_signal(end_b)),
         (scope_done(SubId) <- sub(SubId, 1, inner_c), end_signal(end_c)) ])).

pipeline_run(TickA, TickB, TickC, FinalAll, DeltaTicks) :-
    pipeline_program(Prog),
    pipeline_schedule(TickA, TickB, TickC, Schedule),
    run_program(Prog, [], Schedule, FinalAll, DeltaTicks).

pipeline_schedule(TickA, TickB, TickC, Schedule) :-
    numlist(1, 8, Slots),
    maplist(pipeline_slot(TickA, TickB, TickC), Slots, Schedule).

pipeline_slot(_, _, _, 1, [ subscribe(1, 0, outer, 7), subscribe(2, 1, inner_c, 8) ]) :- !.
pipeline_slot(TickA, TickB, TickC, Slot, Items) :-
    findall(+end_signal(Signal),
            ( member(Signal-SignalTick, [end_a-TickA, end_b-TickB, end_c-TickC]),
              SignalTick == Slot ),
            Signals),
    ( Slot == 2 -> Items = [ fill(inner_c, 8, stage_row(inner_c, value_one)) | Signals ]
    ; Items = Signals ).

scenario(pipeline_example, Prog, [], Schedule) :-
    pipeline_program(Prog), pipeline_schedule(3, 5, 4, Schedule).

% ═══════════════════════════════════════════════════════════════════════════
% SCENARIOS -- Q3: the rx contract as tick items
% ═══════════════════════════════════════════════════════════════════════════
% next        fill(Target, Salt, Row), demand-gated
% error       a fill whose VALUE is the error arm (error-arm-is-a-value)
% complete    a fill whose VALUE is the terminal arm, plus a scope_done rule
% subscribe   subscribe(...)  |  unsubscribe  unsubscribe(...)
% finalize    a departure rule (ruling r4); there is no finalize item
%
% The three departures are graded side by side: the SCOPED VIEW's -deltas are
% identical in all three, and the three-way reason is derived by joining the
% data the scope left behind.

contract_program(
  prog([ kind(source_row/2, log), keep(source_row/2, all),
         kind(closed_row/2, log), keep(closed_row/2, all),
         kind(ended/2, log),      keep(ended/2, all) ],
       [ (live_row(Target, Value) <- demanded(Target, _), source_row(Target, Value)),
         (scope_done(SubId) <- sub(SubId, _, Target), source_row(Target, done)),
         (scope_done(SubId) <- sub(SubId, _, Target), source_row(Target, error(_))),
         (closed_row(Target, Value) <+ only(departed(live_row(Target, Value)))),
         (ended(SubId, complete) <+ only(departed(sub(SubId, _, Target))),
                                    pre(source_row(Target, done))),
         (ended(SubId, error) <+ only(departed(sub(SubId, _, Target))),
                                 pre(source_row(Target, error(_)))),
         (ended(SubId, teardown) <+ only(departed(sub(SubId, _, Target))),
                                    not(source_row(Target, done)),
                                    not(source_row(Target, error(_)))) ])).

scenario(rx_contract(Ending), Prog, [], Schedule) :-
    member(Ending, [complete, error, teardown]),
    contract_program(Prog),
    contract_terminal(Ending, TerminalItems),
    Schedule = [ [ fill(feed_one, 7, source_row(feed_one, too_early)),
                   subscribe(1, 0, feed_one, 7) ],
                 [ fill(feed_one, 7, source_row(feed_one, value_one)) ],
                 TerminalItems,
                 [ ],
                 [ ] ].

contract_terminal(complete, [ fill(feed_one, 7, source_row(feed_one, done)) ]).
contract_terminal(error,    [ fill(feed_one, 7, source_row(feed_one, error(500))) ]).
contract_terminal(teardown, [ unsubscribe(1) ]).

% ═══════════════════════════════════════════════════════════════════════════
% SCENARIOS -- Q4: flattening strategies as ONE policy value
% ═══════════════════════════════════════════════════════════════════════════
% The four runs share every character of program text except the policy word.

policy_program(Policy,
  prog([ kind(open_tab/1, log), keep(open_tab/1, all),
         kind(tab_done/1, log), keep(tab_done/1, all),
         switch_scope(open_tab(TabId), 1, tab(TabId), Policy) ],
       [ (scope_done(SubId) <- sub(SubId, _, tab(TabId)), tab_done(TabId)) ])).

policy_schedule(
  [ [ subscribe(1, 0, panel, 7) ],
    [ +open_tab(tab_a) ],
    [ +open_tab(tab_b) ],
    [ +tab_done(tab_a) ],
    [ +tab_done(tab_b) ] ]).

scenario(policy_run(Policy), Prog, [], Schedule) :-
    member(Policy, [switch, exhaust, merge, concat]),
    policy_program(Policy, Prog),
    policy_schedule(Schedule).

scenario(concat_order, Prog, [],
  [ [ subscribe(1, 0, panel, 7) ],
    [ +open_tab(tab_a) ],
    [ +open_tab(tab_b), +open_tab(tab_c) ],
    [ +tab_done(tab_a) ],
    [ +tab_done(tab_b) ],
    [ +tab_done(tab_c) ] ]) :- policy_program(concat, Prog).

scenario(concat_parent_teardown, Prog, [],
  [ [ subscribe(1, 0, panel, 7) ],
    [ +open_tab(tab_a) ],
    [ +open_tab(tab_b), +open_tab(tab_c) ],
    [ unsubscribe(1) ] ]) :- policy_program(concat, Prog).

% ── Q4b. concat WITHOUT kernel state ────────────────────────────────────────
% exhaust policy + four ordinary rules + one keyed pending register. Queue
% depth one, and the dequeue costs two ticks (death -> departure carry ->
% start row -> carry -> swap) where the kernel drain costs zero.

userland_concat_program(
  prog([ kind(request_tab/2, log), keep(request_tab/2, all),
         kind(start_tab/2, log),   keep(start_tab/2, all),
         kind(tab_done/1, log),    keep(tab_done/1, all),
         keyed(pending/2, [1]),
         switch_scope(start_tab(_SessionId, TabId), 1, tab(TabId), exhaust) ],
       [ (start_tab(SessionId, TabId) <+ only(request_tab(SessionId, TabId)),
                                         not(sub(_, 1, tab(_)))),
         (pending(SessionId, TabId) <+ only(request_tab(SessionId, TabId)),
                                       sub(_, 1, tab(_))),
         (start_tab(SessionId, TabId) <+ only(departed(sub(_, 1, tab(_)))),
                                         pre(pending(SessionId, TabId)),
                                         TabId \== none),
         (pending(SessionId, none) <+ only(departed(sub(_, 1, tab(_)))),
                                      pre(pending(SessionId, _))),
         (scope_done(SubId) <- sub(SubId, _, tab(TabId)), tab_done(TabId)) ])).

scenario(userland_concat, Prog, [ pending(session_one, none) ],
  [ [ subscribe(1, 0, panel, 7) ],
    [ +request_tab(session_one, tab_a) ],
    [ +request_tab(session_one, tab_b) ],
    [ +tab_done(tab_a) ],
    [ ], [ ], [ ] ]) :- userland_concat_program(Prog).

% ═══════════════════════════════════════════════════════════════════════════
% SCENARIOS -- Q5: switch x state machine
% ═══════════════════════════════════════════════════════════════════════════
% The machine is conformance/fixtures/state_machine.pl's, unchanged, plus two
% lines: a switch on the fetching phase, and a scope_done rule with negation.
% Together they are takeUntil expressed as keyed replace: the scope lives
% exactly while the register holds the state.

machine_program(Policy,
  prog([ kind(poll_due/1, log),     keep(poll_due/1, all),
         kind(fetch_result/2, log), keep(fetch_result/2, all),
         kind(fetch_body/2, log),   keep(fetch_body/2, all),
         keyed(phase/2, [1]),
         keyed(retries/2, [1]),
         switch_scope(phase(Endpoint, fetching), 1, fetch_of(Endpoint), Policy) ],
       [ (phase(Endpoint, fetching) <+
            only(poll_due(Endpoint)), pre(phase(Endpoint, idle))),
         (phase(Endpoint, idle) <+
            only(fetch_result(Endpoint, fresh(_, _))), pre(phase(Endpoint, fetching))),
         (phase(Endpoint, idle) <+
            only(fetch_result(Endpoint, unchanged)), pre(phase(Endpoint, fetching))),
         (phase(Endpoint, idle) <+
            only(fetch_result(Endpoint, error(_))), pre(phase(Endpoint, fetching))),
         (retries(Endpoint, Next) <+
            only(fetch_result(Endpoint, error(_))),
            pre(retries(Endpoint, SoFar)), Next := SoFar + 1),
         (retries(Endpoint, 0) <+ only(fetch_result(Endpoint, fresh(_, _)))),
         (fetch_wanted(Endpoint) <- phase(Endpoint, fetching)),
         (scope_done(SubId) <- sub(SubId, _, fetch_of(Endpoint)),
                               not(phase(Endpoint, fetching))) ])).

scenario(state_scope_lifecycle, Prog,
  [ phase(gh_repos, idle), retries(gh_repos, 0) ],
  [ [ subscribe(1, 0, panel, 7), +poll_due(gh_repos) ],
    [ ],
    [ +fetch_result(gh_repos, fresh(tag_v1, body_one)) ],
    [ fill(fetch_of(gh_repos), 1001, fetch_body(gh_repos, late_body)) ],
    [ ] ]) :- machine_program(switch, Prog).

% the flap: error then poll_due in ONE tick. The register leaves fetching and
% comes back inside the tick, so the boundary shows nothing and the carry (the
% switch's input) is empty. Zero teardowns, zero plants, one retry counted.
scenario(state_flap_nets_to_zero, Prog,
  [ phase(gh_repos, idle), retries(gh_repos, 0) ],
  [ [ subscribe(1, 0, panel, 7), +poll_due(gh_repos) ],
    [ ],
    [ +fetch_result(gh_repos, error(500)), +poll_due(gh_repos) ],
    [ ] ]) :- machine_program(switch, Prog).

% the same two events on SEPARATE ticks: one teardown, one fresh plant.
scenario(state_exit_then_reenter, Prog,
  [ phase(gh_repos, idle), retries(gh_repos, 0) ],
  [ [ subscribe(1, 0, panel, 7), +poll_due(gh_repos) ],
    [ ],
    [ +fetch_result(gh_repos, error(500)) ],
    [ ],
    [ +poll_due(gh_repos) ],
    [ ] ]) :- machine_program(switch, Prog).

% TWO keys, ONE parent scope. The state register keys the TARGET; the
% flattening slot is keyed by the PARENT. Under switch policy the two
% endpoints therefore fight over one slot and the loser is planted and torn
% down inside a single tick, invisibly. Under merge policy each key gets its
% own sibling scope and the negated scope_done rule ends them independently.
scenario(two_endpoints_one_slot, Prog, Initial, Schedule) :-
    machine_program(switch, Prog), two_endpoint_session(Initial, Schedule).

scenario(two_endpoints_merged, Prog, Initial, Schedule) :-
    machine_program(merge, Prog), two_endpoint_session(Initial, Schedule).

two_endpoint_session(
  [ phase(gh_repos, idle), phase(gh_issues, idle),
    retries(gh_repos, 0), retries(gh_issues, 0) ],
  [ [ subscribe(1, 0, panel, 7), +poll_due(gh_repos), +poll_due(gh_issues) ],
    [ ],
    [ +fetch_result(gh_repos, unchanged) ],
    [ ] ]).

% ═══════════════════════════════════════════════════════════════════════════
% SCENARIOS -- Q6: MINIMIZE THE KERNEL
% ═══════════════════════════════════════════════════════════════════════════
% Adversarial pass on the 7-item absorption list. sub_lifetimes already found
% that demand is refcounted by IVM SUPPORT rather than by the sub row. Pushed
% all the way: is any forest rel stored because it must be, or because the
% first design stored it?
%
% The minimal-kernel model uses NO switch_scope decl, NO scope_done, NO
% subscribe/unsubscribe items and NO forest rows. A subscription is an
% ordinary keyed Set row of the PROGRAM's own rel, the demand set is an
% ordinary derived relation over it, and teardown is the ordinary retraction
% cascade of that row's support cone.

% ── Q6a. switchMap IS keyed replace ─────────────────────────────────────────
% The scope root row is keyed by the session, so a new route replaces it, so
% demanded/2 loses support, so the scoped view retracts and the in-flight fill
% is refused. No teardown statement exists anywhere in this program.
% The instance column is the program's own counter (the same pre + := scan
% fixtures/state_machine.pl uses for retries) and it is what keeps two
% subscriptions with identical content distinguishable.

derived_switch_program(
  prog([ kind(route_change/2, log), keep(route_change/2, all),
         keyed(open_scope/3, [1]),
         keyed(scope_instance/2, [1]),
         kind(route_row/2, set) ],
       [ (scope_instance(SessionId, Next) <+ only(route_change(SessionId, _)),
                                             pre(scope_instance(SessionId, SoFar)),
                                             Next := SoFar + 1),
         (open_scope(SessionId, Next, route_data(RouteId)) <+
             only(route_change(SessionId, RouteId)),
             pre(scope_instance(SessionId, SoFar)), Next := SoFar + 1),
         (demanded(Target, Instance) <- open_scope(_, Instance, Target)),
         (route_view(RouteId, Body) <- demanded(route_data(RouteId), _),
                                       route_row(RouteId, Body)) ])).

scenario(derived_switch, Prog, [ scope_instance(session_one, 0) ],
  [ [ +route_change(session_one, settings) ],
    [ fill(route_data(settings), 1, route_row(settings, body_settings)) ],
    [ +route_change(session_one, profile) ],
    [ fill(route_data(settings), 1, route_row(settings, stale_body)),
      fill(route_data(profile), 2, route_row(profile, body_profile)) ],
    [ ] ]) :- derived_switch_program(Prog).

% ── Q6b. nested teardown without sub_path ───────────────────────────────────
% The inner scope's liveness JOINS the outer's row, so retracting the outer
% retracts the inner transitively. Materialized paths existed only to make a
% teardown statement cheap; with no teardown statement there is no path.

derived_nesting_program(
  prog([ keyed(open_pane/2, [1]),
         keyed(open_detail/2, [1]),
         kind(select_item/2, log), keep(select_item/2, all),
         kind(detail_row/2, set) ],
       [ (open_detail(PaneId, ItemId) <+ only(select_item(PaneId, ItemId))),
         (live_detail(PaneId, detail(ItemId)) <- open_pane(PaneId, _),
                                                 open_detail(PaneId, ItemId)),
         (demanded(Target, PaneId) <- live_detail(PaneId, Target)),
         (detail_view(ItemId, Body) <- demanded(detail(ItemId), _),
                                       detail_row(ItemId, Body)) ])).

scenario(derived_nesting, Prog, [],
  [ [ +open_pane(pane_one, item_list), +select_item(pane_one, item_a) ],
    [ fill(detail(item_a), pane_one, detail_row(item_a, body_a)) ],
    [ -open_pane(pane_one, item_list) ],
    [ fill(detail(item_a), pane_one, detail_row(item_a, late_body)) ],
    [ ] ]) :- derived_nesting_program(Prog).

% ── Q6c. the flattening policy IS the key declaration ───────────────────────
% Same three rules in all three programs. switch keys the scope row by the
% outer identity so a new value replaces the old; merge adds the value to the
% key so both coexist; exhaust keeps the switch key and adds one guard.

derived_policy_key(switch,  keyed(open_tab/2, [1])).
derived_policy_key(merge,   keyed(open_tab/2, [1, 2])).
derived_policy_key(exhaust, keyed(open_tab/2, [1])).

derived_policy_guard(exhaust, not(live_tab(_))) :- !.
derived_policy_guard(_, true).

derived_policy_program(Policy,
  prog([ kind(open_request/2, log), keep(open_request/2, all),
         kind(tab_closed/1, log),   keep(tab_closed/1, all),
         KeyDecl ],
       [ (open_tab(SessionId, TabId) <+ only(open_request(SessionId, TabId)), Guard),
         (live_tab(TabId) <- open_tab(_, TabId), not(tab_closed(TabId))),
         (demanded(tab(TabId), TabId) <- live_tab(TabId)) ])) :-
    derived_policy_key(Policy, KeyDecl),
    derived_policy_guard(Policy, Guard).

derived_policy_schedule(
  [ [ +open_request(session_one, tab_a) ],
    [ +open_request(session_one, tab_b) ],
    [ +tab_closed(tab_a) ],
    [ ] ]).

scenario(derived_policy(Policy), Prog, [], Schedule) :-
    member(Policy, [switch, merge, exhaust]),
    derived_policy_program(Policy, Prog),
    derived_policy_schedule(Schedule).

% concat = exhaust + the departure replay, with the pending set as an ordinary
% keyed PROGRAM rel. Nothing engine-owned; the queue is the one thing in this
% whole section with no derivation, and it does not have to live in the kernel.
derived_concat_program(
  prog([ kind(open_request/2, log), keep(open_request/2, all),
         kind(tab_closed/1, log),   keep(tab_closed/1, all),
         keyed(open_tab/2, [1]),
         keyed(pending_tab/2, [1]) ],
       [ (open_tab(SessionId, TabId) <+ only(open_request(SessionId, TabId)),
                                        not(live_tab(_))),
         (pending_tab(SessionId, TabId) <+ only(open_request(SessionId, TabId)),
                                           live_tab(_)),
         (open_tab(SessionId, TabId) <+ only(departed(live_tab(_))),
                                        pre(pending_tab(SessionId, TabId)),
                                        TabId \== none),
         (pending_tab(SessionId, none) <+ only(departed(live_tab(_))),
                                          pre(pending_tab(SessionId, _))),
         (live_tab(TabId) <- open_tab(_, TabId), not(tab_closed(TabId))),
         (demanded(tab(TabId), TabId) <- live_tab(TabId)) ])).

scenario(derived_concat, Prog, [ pending_tab(session_one, none) ],
  [ [ +open_request(session_one, tab_a) ],
    [ +open_request(session_one, tab_b) ],
    [ +tab_closed(tab_a) ],
    [ ], [ ] ]) :- derived_concat_program(Prog).

% ── Q6d. self-completion without a settling phase ───────────────────────────
% forkJoin: the scope closes itself when both arms land. Written as an EDGE
% write into a rel strictly upstream of the scope root row, so the negation is
% stratified. Written instead as a level rule over rows produced UNDER the
% scope, the dependency graph closes a negative cycle through the demand edge
% (live -neg-> done -> result -demand-> live) and the program is unstratifiable.

derived_fork_program(
  prog([ kind(result_a/1, log),    keep(result_a/1, all),
         kind(result_b/1, log),    keep(result_b/1, all),
         kind(fork_closed/1, log), keep(fork_closed/1, all),
         keyed(open_fork/2, [1]) ],
       [ (fork_closed(SessionId) <+ result_a(_), result_b(_), open_fork(SessionId, _)),
         (live_fork(SessionId, Target) <- open_fork(SessionId, Target),
                                          not(fork_closed(SessionId))),
         (demanded(Target, SessionId) <- live_fork(SessionId, Target)),
         (fork_view(Value) <- demanded(arm_target, _), result_a(Value)) ])).

scenario(derived_fork, Prog, [],
  [ [ +open_fork(session_one, arm_target) ],
    [ fill(arm_target, session_one, result_a(alpha)) ],
    [ fill(arm_target, session_one, result_b(beta)) ],
    [ fill(arm_target, session_one, result_a(late_alpha)) ],
    [ ] ]) :- derived_fork_program(Prog).

% ── Q6e. the stale-fill trichotomy (ruling downgraded to provisional) ────────
% Three readings, costed in primitives rather than argued:
%   abort-on-teardown  demand deletion IS the cancel; no orphan fill exists
%   orphan-as-a-row    the response lands in an ordinary rel; the VIEW is what
%                      is scoped, so no subscriber sees it until one demands it
%   drop               the gate refuses the fill, which needs PER-INSTANCE
%                      demand identity, which nothing else in the model needs

% content-addressed demand: the key IS the content, no instance column
content_feed_program(
  prog([ kind(open_request/2, log), keep(open_request/2, all),
         kind(cache_row/2, set),
         keyed(open_feed/2, [1]) ],
       [ (open_feed(SessionId, Name) <+ only(open_request(SessionId, Name))),
         (demanded(feed(Name), Name) <- open_feed(_, Name)),
         (feed_view(Name, Body) <- demanded(feed(Name), _), cache_row(Name, Body)) ])).

% the first subscription's response arrives after an IDENTICAL reopen
scenario(content_demand_reopen, Prog, [],
  [ [ +open_request(session_one, alpha) ],
    [ -open_feed(session_one, alpha) ],
    [ +open_request(session_one, alpha) ],
    [ fill(feed(alpha), alpha, cache_row(alpha, first_response)) ],
    [ ] ]) :- content_feed_program(Prog).

% the orphan arrives as an ORDINARY row while nobody is subscribed, and the
% next subscriber reads it without a refetch
scenario(orphan_surfaced_as_a_row, Prog, [],
  [ [ +open_request(session_one, alpha) ],
    [ -open_feed(session_one, alpha) ],
    [ +cache_row(alpha, orphan_body) ],
    [ +open_request(session_two, alpha) ],
    [ ] ]) :- content_feed_program(Prog).

% per-instance demand: the same reopen, with a program-side counter
instance_feed_program(
  prog([ kind(open_request/2, log), keep(open_request/2, all),
         kind(cache_row/2, set),
         keyed(open_feed/3, [1]),
         keyed(feed_counter/2, [1]) ],
       [ (feed_counter(SessionId, Next) <+ only(open_request(SessionId, _)),
                                           pre(feed_counter(SessionId, SoFar)),
                                           Next := SoFar + 1),
         (open_feed(SessionId, Next, Name) <+ only(open_request(SessionId, Name)),
                                              pre(feed_counter(SessionId, SoFar)),
                                              Next := SoFar + 1),
         (demanded(feed(Name), Instance) <- open_feed(_, Instance, Name)),
         (feed_view(Name, Body) <- demanded(feed(Name), _), cache_row(Name, Body)) ])).

scenario(instance_demand_reopen, Prog, [ feed_counter(session_one, 0) ],
  [ [ +open_request(session_one, alpha) ],
    [ -open_feed(session_one, 1, alpha) ],
    [ +open_request(session_one, alpha) ],
    [ fill(feed(alpha), 1, cache_row(alpha, first_response)) ],
    [ fill(feed(alpha), 2, cache_row(alpha, second_response)) ],
    [ ] ]) :- instance_feed_program(Prog).

% abort-on-teardown versus drop: the SAME schedule with and without the orphan
% fill item. If the two stores agree, the two readings are observationally
% identical and the only difference is whether the effect kept running.
orphan_probe_schedule(Presence, Schedule) :-
    orphan_probe_item(Presence, Items),
    Schedule = [ [ +open_request(session_one, alpha) ],
                 [ -open_feed(session_one, 1, alpha) ],
                 Items,
                 [ ] ].

orphan_probe_item(with_orphan, [ fill(feed(alpha), 1, cache_row(alpha, first_response)) ]).
orphan_probe_item(no_orphan, []).

orphan_probe_run(Presence, FinalAll) :-
    instance_feed_program(Prog),
    orphan_probe_schedule(Presence, Schedule),
    run_program(Prog, [ feed_counter(session_one, 0) ], Schedule, FinalAll, _).

% ═══════════════════════════════════════════════════════════════════════════
% CHECKS -- 0. the lattice operators, as reused
% ═══════════════════════════════════════════════════════════════════════════

check(scope_min_is_disjunction_of_end_signals,
  ( until_signal(disconnect, Left), until_signal(document_closed, Right),
    scope_min(Left, Right, Result),
    Result == until([[disconnect], [document_closed]]) )).

check(join_max_is_conjunction_of_end_signals,
  ( until_signal(disconnect, Left), until_signal(outer_next, Right),
    join_max(Left, Right, Result),
    Result == until([[disconnect, outer_next]]) )).

check(formula_first_true_takes_max_within_a_clause,
  ( formula_first_true(until([[end_a, end_b]]), [end_a-2, end_b-5], Answer),
    Answer == 5 )).

check(formula_first_true_takes_min_across_clauses,
  ( formula_first_true(until([[end_c], [end_a, end_b]]),
                       [end_a-2, end_b-5, end_c-3], Answer),
    Answer == 3 )).

check(formula_with_no_satisfied_clause_is_none,
  ( formula_first_true(until([[end_c]]), [end_a-2], Answer), Answer == none )).

% ═══════════════════════════════════════════════════════════════════════════
% CHECKS -- Q1: can the switch itself flow?
% ═══════════════════════════════════════════════════════════════════════════

check(the_register_is_never_an_outside_arrival,
  ( schedule_items(register_drives_the_switch, Items),
    \+ ( member(Item, Items), Item = +Row, rel_ref(Row, current_route/2) ) )).

check(switch_on_a_state_register_fires,
  ( run_prefix(register_drives_the_switch, 3, Final, _),
    rel_rows(sub/3, Final,
             [ sub(1, 0, shell_scope), sub(1001, 1, route_data(session_one, settings)) ]) )).

% The register write and the scope plant are one tick apart, and the gap is
% q4 next_tick: an edge-written row is an occurrence for T+1, never same-tick.
check(register_switch_lands_one_tick_after_the_write,
  ( run_named(register_drives_the_switch, _, DeltaTicks),
    nth1(2, DeltaTicks, SecondTick),
    memberchk(+current_route(session_one, settings), SecondTick),
    \+ ( member(Delta, SecondTick), Delta = +sub(_, _, _) ),
    nth1(3, DeltaTicks, ThirdTick),
    memberchk(+sub(1001, 1, route_data(session_one, settings)), ThirdTick) )).

check(register_switch_swaps_on_each_replace,
  ( run_named(register_drives_the_switch, Final, DeltaTicks),
    rel_rows(sub/3, Final,
             [ sub(1, 0, shell_scope), sub(1002, 1, route_data(session_one, profile)) ]),
    scope_birth_ticks(DeltaTicks, route_data(session_one, settings), [3]),
    scope_birth_ticks(DeltaTicks, route_data(session_one, profile), [5]) )).

check(enum_arm_switch_selects_one_constructor,
  ( run_prefix(enum_arm_drives_the_switch, 3, Final, _),
    rel_rows(sub/3, Final,
             [ sub(1, 0, feed), sub(1001, 1, body_of(gh_repos, tag_v1)) ]) )).

check(non_matching_arms_cause_zero_scope_churn,
  ( run_prefix(enum_arm_drives_the_switch, 2, AfterError, _),
    scope_targets(AfterError, [feed]),
    run_prefix(enum_arm_drives_the_switch, 4, AfterUnchanged, _),
    scope_targets(AfterUnchanged, [feed, body_of(gh_repos, tag_v1)]) )).

check(matching_arm_swaps_the_scope,
  ( run_named(enum_arm_drives_the_switch, Final, DeltaTicks),
    rel_rows(sub/3, Final,
             [ sub(1, 0, feed), sub(1002, 1, body_of(gh_repos, tag_v2)) ]),
    nth1(5, DeltaTicks, FifthTick),
    memberchk(-sub(1001, 1, body_of(gh_repos, tag_v1)), FifthTick),
    memberchk(+sub(1002, 1, body_of(gh_repos, tag_v2)), FifthTick) )).

check(routing_table_decides_the_target_by_rows,
  ( run_named(routing_table_is_data, Final, _),
    scope_targets(Final,
      [ panel, detail_pane(item_a), feed(fast_lane), feed(slow_lane, wide_window) ]) )).

check(one_switch_decl_serves_three_target_shapes,
  ( routing_program(prog(Decls, _)),
    findall(Decl, ( member(Decl, Decls), Decl = switch_scope(_, _, _, _) ), Switches),
    length(Switches, 1),
    run_named(routing_table_is_data, Final, _),
    findall(Ref, ( member(sub(_, 1, Target), Final), rel_ref(Target, Ref) ), TargetRefs),
    msort(TargetRefs, Sorted),
    Sorted == [detail_pane/1, feed/1, feed/2] )).

check(universal_switch_decl_carries_no_program_text,
  ( universal_program(prog(Decls, Rules)),
    Rules == [],
    memberchk(switch_scope(Pattern, ParentScope, TargetTerm, switch), Decls),
    Pattern = switch_to(PatternParent, PatternTarget),
    PatternParent == ParentScope, PatternTarget == TargetTerm )).

check(universal_switch_plants_under_the_parent_named_by_the_row,
  ( run_prefix(universal_switch_decl, 3, Final, _),
    rel_rows(sub/3, Final,
             [ sub(1, 0, root_scope), sub(2, 0, other_scope),
               sub(1001, 1, alpha_target), sub(1002, 2, beta_target) ]) )).

check(universal_switch_replaces_only_the_named_parents_child,
  ( run_named(universal_switch_decl, Final, _),
    rel_rows(sub/3, Final,
             [ sub(1, 0, root_scope), sub(2, 0, other_scope),
               sub(1002, 2, beta_target), sub(1003, 1, gamma_target) ]) )).

check(switch_pattern_variables_are_not_severed,
  ( run_named(two_switches_one_tick, Final, _),
    rel_rows(sub/3, Final,
             [ sub(1, 0, root_scope), sub(2, 0, other_scope),
               sub(1001, 1, alpha_target), sub(1002, 2, beta_target) ]) )).

check(switch_under_a_closed_parent_is_silent,
  ( run_named(switch_under_a_closed_parent, Final, _),
    forest_rows(Final, []) )).

% ═══════════════════════════════════════════════════════════════════════════
% CHECKS -- Q2: what is complete?
% ═══════════════════════════════════════════════════════════════════════════

check(terminal_enum_arm_derives_scope_done,
  ( run_named(completion_from(terminal_arm), Final, DeltaTicks),
    forest_rows(Final, []),
    nth1(3, DeltaTicks, ThirdTick),
    memberchk(-sub(1, 0, feed_one), ThirdTick) )).

check(conjunctive_body_derives_scope_done,
  ( run_named(completion_from(conjunctive_body), Final, DeltaTicks),
    forest_rows(Final, []),
    nth1(3, DeltaTicks, ThirdTick),
    memberchk(-sub(1, 0, feed_one), ThirdTick) )).

check(explicit_rule_head_derives_scope_done,
  ( run_named(completion_from(explicit_head), Final, DeltaTicks),
    forest_rows(Final, []),
    nth1(3, DeltaTicks, ThirdTick),
    memberchk(-sub(1, 0, feed_one), ThirdTick) )).

check(all_three_completion_sources_are_plain_level_rules,
  ( forall(member(Source, [terminal_arm, conjunctive_body, explicit_head]),
           ( completion_rules(Source, Rules),
             forall(member(Rule, Rules), Rule = (scope_done(_) <- _)) )) )).

check(completion_retracts_the_scoped_view_in_the_same_tick,
  ( run_named(completion_from(terminal_arm), Final, DeltaTicks),
    rel_rows(live_row/2, Final, []),
    nth1(3, DeltaTicks, ThirdTick),
    memberchk(-live_row(feed_one, value_one), ThirdTick) )).

check(completed_scope_leaves_its_data_behind,
  ( run_named(completion_from(terminal_arm), Final, _),
    rel_rows(stream_row/2, Final,
             [ stream_row(feed_one, done), stream_row(feed_one, value_one) ]) )).

check(completion_composes_by_join_max_at_runtime,
  ( forall(( member(TickA, [2, 3, 5]), member(TickB, [2, 4, 6]) ),
           ( pipeline_run(TickA, TickB, 99, _, DeltaTicks),
             until_signal(end_a, LifetimeA), until_signal(end_b, LifetimeB),
             join_max(LifetimeA, LifetimeB, OuterLifetime),
             formula_first_true(OuterLifetime,
                                [end_a-TickA, end_b-TickB, end_c-99], Predicted),
             scope_death_tick(DeltaTicks, 1, Observed),
             Observed == Predicted )) )).

check(nesting_composes_by_scope_min_at_runtime,
  ( forall(( member(TickA, [2, 3]), member(TickB, [4, 6]), member(TickC, [3, 5, 99]) ),
           ( pipeline_run(TickA, TickB, TickC, _, DeltaTicks),
             until_signal(end_a, LifetimeA), until_signal(end_b, LifetimeB),
             until_signal(end_c, LifetimeC),
             join_max(LifetimeA, LifetimeB, OuterLifetime),
             scope_min(LifetimeC, OuterLifetime, InnerLifetime),
             formula_first_true(InnerLifetime,
                                [end_a-TickA, end_b-TickB, end_c-TickC], Predicted),
             scope_death_tick(DeltaTicks, 2, Observed),
             Observed == Predicted )) )).

check(the_composed_formula_is_the_one_the_operators_build,
  ( until_signal(end_a, LifetimeA), until_signal(end_b, LifetimeB),
    until_signal(end_c, LifetimeC),
    join_max(LifetimeA, LifetimeB, OuterLifetime),
    OuterLifetime == until([[end_a, end_b]]),
    scope_min(LifetimeC, OuterLifetime, InnerLifetime),
    InnerLifetime == until([[end_a, end_b], [end_c]]) )).

check(inner_scope_dies_with_the_outer_when_its_own_signal_never_fires,
  ( pipeline_run(2, 4, 99, Final, DeltaTicks),
    forest_rows(Final, []),
    scope_death_tick(DeltaTicks, 1, Outer), Outer == 4,
    scope_death_tick(DeltaTicks, 2, Inner), Inner == 4 )).

check(inner_scope_can_die_first_without_ending_the_outer,
  ( pipeline_run(2, 6, 3, _, DeltaTicks),
    scope_death_tick(DeltaTicks, 2, Inner), Inner == 3,
    scope_death_tick(DeltaTicks, 1, Outer), Outer == 6 )).

check(completion_cascade_settles_inside_one_tick,
  ( pipeline_run(2, 4, 99, _, DeltaTicks),
    nth1(4, DeltaTicks, FourthTick),
    memberchk(-sub(1, 0, outer), FourthTick),
    memberchk(-sub(2, 1, inner_c), FourthTick),
    memberchk(-stage_view(inner_c, value_one), FourthTick) )).

% ═══════════════════════════════════════════════════════════════════════════
% CHECKS -- Q3: the rx contract as tick items
% ═══════════════════════════════════════════════════════════════════════════

check(next_is_a_demand_gated_fill,
  ( run_named(rx_contract(complete), Final, _),
    \+ member(source_row(feed_one, too_early), Final),
    member(source_row(feed_one, value_one), Final) )).

check(error_is_a_value_row_and_never_an_item,
  ( scenario(rx_contract(error), _, _, Schedule),
    append(Schedule, Items),
    forall(member(Item, Items),
           ( Item = fill(_, _, _) ; Item = subscribe(_, _, _, _) ; Item = unsubscribe(_) )),
    run_named(rx_contract(error), Final, _),
    member(source_row(feed_one, error(500)), Final) )).

check(complete_is_a_terminal_row_plus_a_level_rule,
  ( contract_program(prog(_, Rules)),
    memberchk((scope_done(_) <- sub(_, _, _), source_row(_, done)), Rules),
    run_named(rx_contract(complete), Final, _),
    forest_rows(Final, []) )).

check(unsubscribe_is_the_only_lifetime_item_the_outside_sends,
  ( contract_terminal(teardown, [ unsubscribe(1) ]),
    run_named(rx_contract(teardown), Final, _),
    forest_rows(Final, []) )).

check(finalize_is_a_departure_rule_not_an_item,
  ( forall(member(Ending, [complete, error, teardown]),
           ( run_named(rx_contract(Ending), Final, _),
             rel_rows(closed_row/2, Final, [ closed_row(feed_one, value_one) ]) )) )).

check(departure_cannot_distinguish_error_complete_teardown,
  ( run_named(rx_contract(complete), _, CompleteTicks),
    run_named(rx_contract(error), _, ErrorTicks),
    run_named(rx_contract(teardown), _, TeardownTicks),
    rel_deltas(live_row/2, CompleteTicks, CompleteDeltas),
    rel_deltas(live_row/2, ErrorTicks, ErrorDeltas),
    rel_deltas(live_row/2, TeardownTicks, TeardownDeltas),
    CompleteDeltas == ErrorDeltas,
    ErrorDeltas == TeardownDeltas,
    nth1(3, CompleteDeltas, ThirdTick),
    ThirdTick == [ -live_row(feed_one, value_one) ] )).

check(scope_death_is_an_ordinary_set_row_departure,
  ( run_named(rx_contract(complete), _, DeltaTicks),
    nth1(3, DeltaTicks, ThirdTick),
    memberchk(-sub(1, 0, feed_one), ThirdTick) )).

check(the_three_way_ending_is_derivable_by_joining_the_data,
  ( run_named(rx_contract(complete), CompleteFinal, _),
    rel_rows(ended/2, CompleteFinal, [ ended(1, complete) ]),
    run_named(rx_contract(error), ErrorFinal, _),
    rel_rows(ended/2, ErrorFinal, [ ended(1, error) ]),
    run_named(rx_contract(teardown), TeardownFinal, _),
    rel_rows(ended/2, TeardownFinal, [ ended(1, teardown) ]) )).

check(errored_scope_keeps_the_error_row,
  ( run_named(rx_contract(error), Final, _),
    rel_rows(source_row/2, Final,
             [ source_row(feed_one, value_one), source_row(feed_one, error(500)) ]) )).

check(torn_down_scope_leaves_no_terminal_row,
  ( run_named(rx_contract(teardown), Final, _),
    rel_rows(source_row/2, Final, [ source_row(feed_one, value_one) ]) )).

check(all_three_endings_empty_the_forest,
  ( forall(member(Ending, [complete, error, teardown]),
           ( run_named(rx_contract(Ending), Final, _), forest_rows(Final, []) )) )).

% ═══════════════════════════════════════════════════════════════════════════
% CHECKS -- Q4: flattening strategies as one policy value
% ═══════════════════════════════════════════════════════════════════════════

check(switch_policy_replaces_the_live_scope,
  ( run_prefix(policy_run(switch), 3, Final, _),
    scope_targets(Final, [panel, tab(tab_b)]) )).

check(exhaust_policy_ignores_arrivals_while_a_scope_lives,
  ( run_prefix(policy_run(exhaust), 3, Final, _),
    scope_targets(Final, [panel, tab(tab_a)]) )).

check(exhaust_policy_drops_the_ignored_value_permanently,
  ( run_named(policy_run(exhaust), Final, _),
    scope_targets(Final, [panel]) )).

check(merge_policy_runs_scopes_in_parallel,
  ( run_prefix(policy_run(merge), 3, Final, _),
    scope_targets(Final, [panel, tab(tab_a), tab(tab_b)]) )).

check(concat_policy_holds_the_second_value_in_a_queue,
  ( run_prefix(policy_run(concat), 3, Final, _),
    scope_targets(Final, [panel, tab(tab_a)]),
    rel_rows(scope_queue/4, Final, [ scope_queue(1001, 1, [1], tab(tab_b)) ]) )).

check(concat_queue_drains_on_completion_in_the_same_tick,
  ( run_named(concat_order, _, DeltaTicks),
    nth1(4, DeltaTicks, FourthTick),
    memberchk(-sub(1001, 1, tab(tab_a)), FourthTick),
    memberchk(+sub(1002, 1, tab(tab_b)), FourthTick),
    memberchk(-scope_queue(1001, 1, [1], tab(tab_b)), FourthTick) )).

check(concat_queue_serves_in_arrival_order,
  ( run_prefix(concat_order, 4, AfterFirst, _),
    scope_targets(AfterFirst, [panel, tab(tab_b)]),
    run_prefix(concat_order, 5, AfterSecond, _),
    scope_targets(AfterSecond, [panel, tab(tab_c)]),
    run_named(concat_order, Final, _),
    scope_targets(Final, [panel]) )).

check(concat_queue_ids_are_dense_integers,
  ( run_prefix(concat_order, 3, Final, _),
    findall(QueueId, member(scope_queue(QueueId, _, _, _), Final), QueueIds),
    msort(QueueIds, Sorted),
    Sorted == [1001, 1002],
    all_integers(Sorted) )).

check(concat_queue_rows_die_with_the_parent_scope,
  ( run_named(concat_parent_teardown, Final, _),
    forest_rows(Final, []),
    rel_rows(scope_queue/4, Final, []) )).

check(the_four_policies_differ_only_in_the_policy_word,
  ( forall(member(Policy, [switch, exhaust, merge, concat]),
           ( policy_program(Policy, prog(Decls, Rules)),
             Rules =@= [ (scope_done(SubId) <- sub(SubId, _, tab(TabId)), tab_done(TabId)) ],
             memberchk(switch_scope(open_tab(_), 1, tab(_), Policy), Decls) )),
    run_prefix(policy_run(switch), 3, SwitchFinal, _),
    run_prefix(policy_run(exhaust), 3, ExhaustFinal, _),
    run_prefix(policy_run(merge), 3, MergeFinal, _),
    run_prefix(policy_run(concat), 3, ConcatFinal, _),
    scope_targets(SwitchFinal, SwitchTargets),
    scope_targets(ExhaustFinal, ExhaustTargets),
    scope_targets(MergeFinal, MergeTargets),
    scope_targets(ConcatFinal, ConcatTargets),
    SwitchTargets \== ExhaustTargets,
    MergeTargets \== ConcatTargets,
    ExhaustTargets == ConcatTargets )).

check(concat_is_reproducible_without_kernel_state,
  ( run_prefix(userland_concat, 3, AfterQueue, _),
    scope_targets(AfterQueue, [panel, tab(tab_a)]),
    rel_rows(pending/2, AfterQueue, [ pending(session_one, tab_b) ]),
    run_named(userland_concat, Final, _),
    scope_targets(Final, [panel, tab(tab_b)]),
    rel_rows(pending/2, Final, [ pending(session_one, none) ]) )).

check(userland_concat_costs_two_ticks_the_kernel_queue_costs_zero,
  ( run_named(userland_concat, _, UserlandTicks),
    scope_death_tick(UserlandTicks, 1001, DeathTick), DeathTick == 4,
    scope_birth_ticks(UserlandTicks, tab(tab_b), [BirthTick]), BirthTick == 6,
    run_named(concat_order, _, KernelTicks),
    scope_death_tick(KernelTicks, 1001, KernelDeath), KernelDeath == 4,
    scope_birth_ticks(KernelTicks, tab(tab_b), [KernelBirth]), KernelBirth == 4 )).

% ═══════════════════════════════════════════════════════════════════════════
% CHECKS -- Q5: switch x state machine
% ═══════════════════════════════════════════════════════════════════════════

check(state_register_scope_lives_exactly_while_the_state_holds,
  ( run_named(state_scope_lifecycle, Final, DeltaTicks),
    scope_birth_ticks(DeltaTicks, fetch_of(gh_repos), [2]),
    scope_death_tick(DeltaTicks, 1001, DeathTick), DeathTick == 3,
    nth1(2, DeltaTicks, SecondTick),
    memberchk(+demanded(fetch_of(gh_repos), 1001), SecondTick),
    scope_targets(Final, [panel]) )).

check(leaving_the_state_tears_the_scope_down_in_the_same_tick,
  ( run_named(state_scope_lifecycle, _, DeltaTicks),
    nth1(3, DeltaTicks, ThirdTick),
    memberchk(-sub(1001, 1, fetch_of(gh_repos)), ThirdTick),
    memberchk(-phase(gh_repos, fetching), ThirdTick),
    memberchk(+phase(gh_repos, idle), ThirdTick) )).

check(take_until_is_keyed_replace_plus_a_negated_scope_done,
  ( machine_program(switch, prog(_, Rules)),
    memberchk((scope_done(_) <- sub(_, _, fetch_of(_)), not(phase(_, fetching))), Rules) )).

check(state_scope_teardown_drops_the_in_flight_fetch_fill,
  ( run_named(state_scope_lifecycle, Final, _),
    rel_rows(fetch_body/2, Final, []) )).

check(same_tick_state_flap_nets_to_zero_scope_churn,
  ( run_named(state_flap_nets_to_zero, Final, DeltaTicks),
    scope_targets(Final, [panel, fetch_of(gh_repos)]),
    rel_deltas(sub/3, DeltaTicks, SubDeltas),
    nth1(3, SubDeltas, ThirdTick), ThirdTick == [],
    rel_deltas(phase/2, DeltaTicks, PhaseDeltas),
    nth1(3, PhaseDeltas, ThirdPhaseTick), ThirdPhaseTick == [] )).

check(the_flap_still_counts_its_retry,
  ( run_named(state_flap_nets_to_zero, Final, _),
    rel_rows(retries/2, Final, [ retries(gh_repos, 1) ]) )).

check(the_flap_moves_the_demand_view_not_at_all,
  ( run_named(state_flap_nets_to_zero, _, DeltaTicks),
    rel_deltas(fetch_wanted/1, DeltaTicks, WantedDeltas),
    nth1(3, WantedDeltas, ThirdTick), ThirdTick == [] )).

check(the_same_two_events_on_separate_ticks_cost_one_teardown,
  ( run_named(state_exit_then_reenter, Final, DeltaTicks),
    scope_targets(Final, [panel, fetch_of(gh_repos)]),
    scope_death_tick(DeltaTicks, 1001, DeathTick), DeathTick == 3,
    scope_birth_ticks(DeltaTicks, fetch_of(gh_repos), BirthTicks),
    BirthTicks == [2, 6] )).

% The state register keys the target; the flattening SLOT is keyed by the
% parent scope. Under switch policy two endpoints share one slot, and the
% first plant is torn down inside the same tick it was made, so the boundary
% never sees it at all.
check(one_parent_scope_is_one_flattening_slot,
  ( run_named(two_endpoints_one_slot, Final, DeltaTicks),
    scope_targets(Final, [panel, fetch_of(gh_issues)]),
    scope_birth_ticks(DeltaTicks, fetch_of(gh_repos), []) )).

check(merge_policy_gives_each_state_key_its_own_scope,
  ( run_named(two_endpoints_merged, _, DeltaTicks),
    scope_birth_ticks(DeltaTicks, fetch_of(gh_repos), [2]),
    scope_birth_ticks(DeltaTicks, fetch_of(gh_issues), [2]) )).

check(one_endpoint_leaving_the_state_leaves_the_other_alone,
  ( run_named(two_endpoints_merged, Final, DeltaTicks),
    scope_targets(Final, [panel, fetch_of(gh_issues)]),
    scope_death_tick(DeltaTicks, 1001, DeathTick), DeathTick == 3 )).

% ── the receipt for ambiguity 3: phase 0 runs before the occurrence pass ─────
check(a_fill_in_the_same_tick_as_its_switch_is_refused,
  ( run_named(state_scope_lifecycle, Final, _),
    \+ member(fetch_body(gh_repos, late_body), Final),
    schedule_items(state_scope_lifecycle, Items),
    memberchk(fill(fetch_of(gh_repos), 1001, fetch_body(gh_repos, late_body)), Items) )).

% ═══════════════════════════════════════════════════════════════════════════
% CHECKS -- Q6: minimize the kernel
% ═══════════════════════════════════════════════════════════════════════════

check(the_derived_model_declares_no_engine_construct,
  ( forall(member(Program,
             [ derived_switch, derived_nesting, derived_concat, derived_fork,
               derived_policy(switch), derived_policy(merge), derived_policy(exhaust) ]),
           ( scenario(Program, prog(Decls, Rules), _, Schedule),
             \+ ( member(Decl, Decls), Decl = switch_scope(_, _, _, _) ),
             \+ ( member(Rule, Rules), Rule = (scope_done(_) <- _) ),
             append(Schedule, Items),
             \+ ( member(Item, Items),
                  ( Item = subscribe(_, _, _, _)
                  ; Item = unsubscribe(_)
                  ; Item = complete(_) ) ) )) )).

check(the_derived_model_stores_no_forest_row,
  ( forall(member(Program, [derived_switch, derived_nesting, derived_concat, derived_fork]),
           ( run_named(Program, Final, _), forest_rows(Final, []) )) )).

check(the_program_owns_its_demand_rule_so_the_engine_injects_none,
  ( derived_switch_program(prog(_, Rules)),
    memberchk((demanded(_, _) <- open_scope(_, _, _)), Rules) )).

check(keyed_replace_alone_is_switch_map,
  ( run_named(derived_switch, Final, DeltaTicks),
    rel_deltas(route_view/2, DeltaTicks,
               [ [], [ +route_view(settings, body_settings) ],
                 [ -route_view(settings, body_settings) ],
                 [ +route_view(profile, body_profile) ], [] ]),
    rel_rows(route_view/2, Final, [ route_view(profile, body_profile) ]) )).

check(the_derived_scope_retracts_its_demand_with_no_teardown_statement,
  ( run_named(derived_switch, _, DeltaTicks),
    nth1(3, DeltaTicks, ThirdTick),
    memberchk(-demanded(route_data(settings), 1), ThirdTick),
    memberchk(+demanded(route_data(profile), 2), ThirdTick) )).

check(the_instance_column_is_ordinary_program_state,
  ( run_named(derived_switch, Final, _),
    rel_rows(scope_instance/2, Final, [ scope_instance(session_one, 2) ]),
    rel_rows(open_scope/3, Final,
             [ open_scope(session_one, 2, route_data(profile)) ]) )).

check(the_derived_gate_still_refuses_the_stale_fill,
  ( run_named(derived_switch, Final, _),
    \+ member(route_row(settings, stale_body), Final),
    member(route_row(settings, body_settings), Final),
    member(route_row(profile, body_profile), Final) )).

check(outer_retraction_cascades_to_the_inner_scope,
  ( run_named(derived_nesting, Final, DeltaTicks),
    rel_deltas(detail_view/2, DeltaTicks,
               [ [], [ +detail_view(item_a, body_a) ],
                 [ -detail_view(item_a, body_a) ], [], [] ]),
    rel_rows(detail_view/2, Final, []),
    rel_rows(live_detail/2, Final, []) )).

check(no_path_row_is_needed_for_nested_teardown,
  ( derived_nesting_program(prog(Decls, Rules)),
    \+ ( member(Decl, Decls), functor(Decl, sub_path, _) ),
    \+ ( member(Rule, Rules), Rule = (sub_path(_, _) <- _) ),
    run_named(derived_nesting, Final, _),
    rel_rows(sub_path/2, Final, []) )).

check(data_written_under_a_derived_scope_survives_it,
  ( run_named(derived_nesting, Final, _),
    rel_rows(detail_row/2, Final, [ detail_row(item_a, body_a) ]),
    rel_rows(open_detail/2, Final, [ open_detail(pane_one, item_a) ]) )).

check(switch_is_the_scope_row_keyed_by_the_outer_identity,
  ( run_prefix(derived_policy(switch), 2, Final, _),
    rel_rows(live_tab/1, Final, [ live_tab(tab_b) ]) )).

check(merge_is_the_same_rules_with_the_value_added_to_the_key,
  ( run_prefix(derived_policy(merge), 2, Final, _),
    rel_rows(live_tab/1, Final, [ live_tab(tab_a), live_tab(tab_b) ]) )).

check(exhaust_is_the_switch_key_plus_one_guard,
  ( run_prefix(derived_policy(exhaust), 2, Final, _),
    rel_rows(live_tab/1, Final, [ live_tab(tab_a) ]),
    run_named(derived_policy(exhaust), FinalAll, _),
    rel_rows(live_tab/1, FinalAll, []) )).

check(the_three_derived_policies_share_every_rule,
  ( derived_policy_program(switch, prog(_, SwitchRules)),
    derived_policy_program(merge, prog(_, MergeRules)),
    SwitchRules =@= MergeRules,
    derived_policy_key(switch, keyed(open_tab/2, [1])),
    derived_policy_key(merge, keyed(open_tab/2, [1, 2])) )).

check(concat_is_exhaust_plus_the_departure_replay,
  ( run_named(derived_concat, Final, DeltaTicks),
    rel_deltas(live_tab/1, DeltaTicks,
               [ [ +live_tab(tab_a) ], [], [ -live_tab(tab_a) ],
                 [ +live_tab(tab_b) ], [] ]),
    rel_rows(pending_tab/2, Final, [ pending_tab(session_one, none) ]) )).

check(self_completion_needs_no_settling_phase,
  ( run_named(derived_fork, Final, DeltaTicks),
    nth1(3, DeltaTicks, ThirdTick),
    memberchk(+fork_closed(session_one), ThirdTick),
    memberchk(-live_fork(session_one, arm_target), ThirdTick),
    memberchk(-demanded(arm_target, session_one), ThirdTick),
    rel_rows(live_fork/2, Final, []) )).

% The negated rel is a Log rel written by an EDGE rule and headed by no level
% rule, so it sits strictly below live_fork and the program is stratified. The
% tempting alternative, deriving the completion condition as a level rule over
% rows produced UNDER the scope, closes a negative cycle through the demand
% edge (live -neg-> done -> result -demand-> live) and is unstratifiable.
check(self_completion_negation_is_stratified_by_construction,
  ( derived_fork_program(prog(Decls, Rules)),
    memberchk((live_fork(_, _) <- open_fork(_, _), not(fork_closed(_))), Rules),
    memberchk(kind(fork_closed/1, log), Decls),
    memberchk((fork_closed(_) <+ _), Rules),
    \+ ( member(Rule, Rules), Rule = (fork_closed(_) <- _) ) )).

check(the_derived_concat_dequeue_costs_one_tick_not_two,
  ( run_named(derived_concat, _, DerivedTicks),
    rel_deltas(live_tab/1, DerivedTicks, DerivedDeltas),
    nth1(3, DerivedDeltas, [ -live_tab(tab_a) ]),
    nth1(4, DerivedDeltas, [ +live_tab(tab_b) ]),
    run_named(userland_concat, _, ForestTicks),
    scope_death_tick(ForestTicks, 1001, ForestDeath), ForestDeath == 4,
    scope_birth_ticks(ForestTicks, tab(tab_b), [ForestBirth]), ForestBirth == 6 )).

check(self_completion_leaves_the_arms_and_drops_the_late_one,
  ( run_named(derived_fork, Final, _),
    rel_rows(result_a/1, Final, [ result_a(alpha) ]),
    rel_rows(result_b/1, Final, [ result_b(beta) ]),
    rel_rows(fork_view/1, Final, []) )).

check(content_addressed_demand_cannot_detect_a_stale_fill,
  ( run_named(content_demand_reopen, Final, _),
    member(cache_row(alpha, first_response), Final) )).

check(an_instance_column_restores_stale_detection,
  ( run_named(instance_demand_reopen, Final, _),
    \+ member(cache_row(alpha, first_response), Final),
    member(cache_row(alpha, second_response), Final) )).

check(an_orphan_admitted_as_a_row_is_reused_by_the_next_subscriber,
  ( run_named(orphan_surfaced_as_a_row, Final, DeltaTicks),
    rel_deltas(feed_view/2, DeltaTicks,
               [ [], [], [], [ +feed_view(alpha, orphan_body) ], [] ]),
    rel_rows(feed_view/2, Final, [ feed_view(alpha, orphan_body) ]),
    rel_rows(cache_row/2, Final, [ cache_row(alpha, orphan_body) ]) )).

check(abort_on_teardown_and_drop_are_indistinguishable_in_the_store,
  ( orphan_probe_run(with_orphan, WithOrphan),
    orphan_probe_run(no_orphan, NoOrphan),
    WithOrphan == NoOrphan )).

go :- run(check).

% ═══════════════════════════════════════════════════════════════════════════
% TRACE PRINTER (receipts for the .md)
% ═══════════════════════════════════════════════════════════════════════════

report :-
    forall(scenario(Name, _, _, _),
           ( format("~w~n", [Name]),
             (   catch(run_named(Name, Final, DeltaTicks), Thrown, true)
             ->  (   var(Thrown)
                 ->  forall(nth1(Index, DeltaTicks, Deltas),
                            format("  tick ~w  ~q~n", [Index, Deltas])),
                     forest_rows(Final, Forest),
                     format("  forest   ~q~n", [Forest])
                 ;   format("  REJECTED ~q~n", [Thrown]) )
             ;   format("  no solution~n", []) ),
             nl )).
