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
            rel_rows/3, rel_deltas/3, json_canon/2 ]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(library(ordsets)).
:- use_module(library(pairs)).
:- use_module(rulings).

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

rel_ref(Atom, Name/Arity) :- functor(Atom, Name, Arity).

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
           throw(aggregate_in_edge_head)).

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

% ═══ expressions ════════════════════════════════════════════════════════════
% Evaluation is the default; goals run left to right: an atom binds, := / is
% computes, a comparison filters. / truncates (Int-only law). concat is the
% interpolation lowering target.

eval_expr(Value, _) :- var(Value), !, throw(unbound_in_expression).
eval_expr(Number, Number) :- number(Number), !.
eval_expr(concat(Parts), Out) :- !,
    maplist(eval_expr, Parts, Values),
    maplist(text_piece, Values, Pieces),
    atomic_list_concat(Pieces, Out).
eval_expr(Left + Right, Out)   :- !, eval_int2(Left, Right, LeftV, RightV), Out is LeftV + RightV.
eval_expr(Left - Right, Out)   :- !, eval_int2(Left, Right, LeftV, RightV), Out is LeftV - RightV.
eval_expr(Left * Right, Out)   :- !, eval_int2(Left, Right, LeftV, RightV), Out is LeftV * RightV.
eval_expr(Left / Right, Out)   :- !, eval_int2(Left, Right, LeftV, RightV), Out is LeftV // RightV.
eval_expr(Left mod Right, Out) :- !, eval_int2(Left, Right, LeftV, RightV), Out is LeftV mod RightV.
eval_expr(Braces, Canon) :- Braces = {}(_), !, json_canon(Braces, Canon).
eval_expr([Head | Tail], Canon) :- !, json_canon([Head | Tail], Canon).
eval_expr(Value, Value).

eval_int2(Left, Right, LeftV, RightV) :-
    eval_expr(Left, LeftV), eval_expr(Right, RightV),
    ( integer(LeftV), integer(RightV) -> true ; throw(arith_on_non_int(LeftV, RightV)) ).

text_piece(Value, Value) :- atomic(Value), !.
text_piece(Value, _) :- throw(non_display_in_concat(Value)).

comparison_goal(_ < _). comparison_goal(_ =< _). comparison_goal(_ > _).
comparison_goal(_ >= _). comparison_goal(_ == _). comparison_goal(_ \== _).

solve_comparison(Left < Right)   :- eval_int2(Left, Right, LeftV, RightV), LeftV < RightV.
solve_comparison(Left =< Right)  :- eval_int2(Left, Right, LeftV, RightV), LeftV =< RightV.
solve_comparison(Left > Right)   :- eval_int2(Left, Right, LeftV, RightV), LeftV > RightV.
solve_comparison(Left >= Right)  :- eval_int2(Left, Right, LeftV, RightV), LeftV >= RightV.
solve_comparison(Left == Right)  :- eval_expr(Left, LeftV), eval_expr(Right, RightV), LeftV == RightV.
solve_comparison(Left \== Right) :- eval_expr(Left, LeftV), eval_expr(Right, RightV), LeftV \== RightV.

% ═══ json ═══════════════════════════════════════════════════════════════════

json_canon(Braces, obj(Sorted)) :- nonvar(Braces), Braces = {}(Fields), !,
    braces_pairs(Fields, Pairs),
    keysort(Pairs, Sorted),
    pairs_keys(Sorted, Keys),
    ( sort(Keys, Distinct), length(Keys, N), length(Distinct, N)
    -> true ; throw(json_dup_key(Keys)) ).
json_canon(List, Canon) :- is_list(List), !, maplist(json_canon, List, Canon).
json_canon(obj(Pairs), obj(Canon)) :- !,
    findall(Key-Value, ( member(Key-Raw, Pairs), json_canon(Raw, Value) ), Canon0),
    keysort(Canon0, Canon).
json_canon(Value, Value).

braces_pairs((Left, Right), Pairs) :- !,
    braces_pairs(Left, LeftPairs), braces_pairs(Right, RightPairs),
    append(LeftPairs, RightPairs, Pairs).
braces_pairs(Key: Raw, [Key-Value]) :- json_canon(Raw, Value).

% decode: open object patterns, holes bind canonical values.
json_decode(Value, Pattern) :- var(Pattern), !, Pattern = Value.
json_decode(obj(Pairs), Pattern) :- nonvar(Pattern), Pattern = {}(Fields), !,
    braces_decode(Fields, Pairs).
json_decode(List, Pattern) :- is_list(Pattern), !,
    is_list(List),
    maplist(json_decode_flip, Pattern, List).
json_decode(Value, Pattern) :- Value = Pattern.

json_decode_flip(Pattern, Value) :- json_decode(Value, Pattern).

braces_decode((Left, Right), Pairs) :- !,
    braces_decode(Left, Pairs), braces_decode(Right, Pairs).
braces_decode(Key: Pattern, Pairs) :-
    memberchk(Key-Value, Pairs),
    Value \== none,
    json_decode(Value, Pattern).

% ═══ body solving ═══════════════════════════════════════════════════════════
% ctx(Visible, PreState, Tick): Visible = rows body atoms read; PreState =
% evolving pre rows; Tick = the phantom clock for now/1.

solve(true, _) :- !.
solve((Left, Right), Ctx) :- !, solve(Left, Ctx), solve(Right, Ctx).
solve(not(Goal), Ctx) :- !, \+ solve(Goal, Ctx).
solve(only(Atom), Ctx) :- !, Ctx = ctx(Visible, _, _), member(Atom, Visible).
solve(pre(Atom), Ctx) :- !, Ctx = ctx(_, PreState, _), member(Atom, PreState).
solve(now(Tick), Ctx) :- !, Ctx = ctx(_, _, Tick).
solve(Variable := Expr, _) :- !, eval_expr(Expr, Value), Variable = Value.
solve(Variable is Expr, _)  :- !, eval_expr(Expr, Value), Variable = Value.
solve(decode(Expr, Pattern), _) :- !,
    eval_expr(Expr, Value), json_decode(Value, Pattern).
solve(json_each(Expr, Element), _) :- !,
    eval_expr(Expr, List), is_list(List), member(Element, List).
solve(Comparison, _) :- comparison_goal(Comparison), !, solve_comparison(Comparison).
solve(Atom, Ctx) :- Ctx = ctx(Visible, _, _), member(Atom, Visible).

body_atoms((Left, Right), Atoms) :- !,
    body_atoms(Left, LeftAtoms), body_atoms(Right, RightAtoms),
    append(LeftAtoms, RightAtoms, Atoms).
body_atoms(only(Atom), [Atom]) :- !.
body_atoms(pre(_), []) :- !.
body_atoms(not(_), []) :- !.
body_atoms(now(_), []) :- !.
body_atoms(true, []) :- !.
body_atoms(_ := _, []) :- !.
body_atoms(_ is _, []) :- !.
body_atoms(decode(_, _), []) :- !.
body_atoms(json_each(_, _), []) :- !.
body_atoms(Goal, []) :- comparison_goal(Goal), !.
body_atoms(Atom, [Atom]).

% q6: markers narrow the trigger set; an unmarked body keeps any-atom.
trigger_atoms(Body, Triggers) :-
    marked_atoms(Body, Marked),
    ( Marked == [] -> body_atoms(Body, Triggers) ; Triggers = Marked ).

marked_atoms((Left, Right), Atoms) :- !,
    marked_atoms(Left, LeftAtoms), marked_atoms(Right, RightAtoms),
    append(LeftAtoms, RightAtoms, Atoms).
marked_atoms(only(Atom), [Atom]) :- !.
marked_atoms(_, []).

% ═══ level closure with aggregates ══════════════════════════════════════════
% Plain rules run to fixpoint; aggregate rules recompute over the result; the
% two alternate until stable (fixtures are stratified).

aggregate_head(Head, Template, Ref) :-
    Head =.. [Name | Args],
    length(Args, Arity), Ref = Name/Arity,
    maplist(classify_head_arg, Args, Template),
    memberchk(agg(_, _), Template).

classify_head_arg(Arg, agg(Kind, Expr)) :-
    nonvar(Arg), Arg =.. [Kind, Expr],
    memberchk(Kind, [count, sum, min, max, json_array]), !.
classify_head_arg(Arg, agg(json_object, KeyExpr-ValueExpr)) :-
    nonvar(Arg), Arg = json_object(KeyExpr, ValueExpr), !.
classify_head_arg(Arg, plain(Arg)).

split_rules(Rules, AggRules, PlainLevel, EdgeRules) :-
    findall(Rule, ( member(Rule, Rules), Rule = (Head <- _), aggregate_head(Head, _, _) ),
            AggRules),
    findall(Rule, ( member(Rule, Rules), Rule = (Head <- _), \+ aggregate_head(Head, _, _) ),
            PlainLevel),
    findall(Rule, ( member(Rule, Rules), Rule = (_ <+ _) ), EdgeRules).

level_closure(PlainLevel, AggRules, Base, Tick, Level) :-
    plain_fixpoint(PlainLevel, Base, Tick, [], Level0),
    agg_loop(PlainLevel, AggRules, Base, Tick, Level0, Level).

plain_fixpoint(PlainLevel, Base, Tick, Known0, Level) :-
    append(Base, Known0, Visible),
    findall(EvaluatedHead,
            ( member((Head <- Body), PlainLevel),
              solve(Body, ctx(Visible, [], Tick)),
              eval_head(Head, EvaluatedHead) ),
            Heads),
    append(Known0, Heads, Merged0),
    sort(Merged0, Merged),
    ( Merged == Known0 -> Level = Known0
    ; plain_fixpoint(PlainLevel, Base, Tick, Merged, Level) ).

% Head values are expressions (named-column rule); evaluate after the joins.
eval_head(Head, Evaluated) :-
    Head =.. [Name | Args],
    maplist(eval_expr, Args, Values),
    Evaluated =.. [Name | Values].

agg_loop(PlainLevel, AggRules, Base, Tick, Known0, Level) :-
    append(Base, Known0, Visible),
    findall(Row, ( member(Rule, AggRules), agg_rule_rows(Rule, Visible, Tick, Row) ), AggRows),
    append(Known0, AggRows, Merged0),
    sort(Merged0, Merged),
    ( Merged == Known0 -> Level = Known0
    ; plain_fixpoint(PlainLevel, Base, Tick, Merged, Widened),
      agg_loop(PlainLevel, AggRules, Base, Tick, Widened, Level) ).

agg_rule_rows((Head <- Body), Visible, Tick, Row) :-
    aggregate_head(Head, Template, Ref),
    Ref = Name/_,
    findall(Contribution,
            ( solve(Body, ctx(Visible, [], Tick)),
              maplist(head_arg_value, Template, Contribution) ),
            Bag),
    Bag \== [],
    findall(GroupKey, ( member(Solution, Bag), group_key(Template, Solution, GroupKey) ), Keys0),
    sort(Keys0, GroupKeys),
    member(GroupKey, GroupKeys),
    findall(Solution, ( member(Solution, Bag), group_key(Template, Solution, GroupKey) ), Group),
    aggregate_args(Template, Group, Args),
    Row =.. [Name | Args].

head_arg_value(plain(Expr), value(Value)) :- eval_expr(Expr, Value).
head_arg_value(agg(json_object, KeyExpr-ValueExpr), contrib(Key-Value)) :- !,
    eval_expr(KeyExpr, Key), eval_expr(ValueExpr, Value).
head_arg_value(agg(_, Expr), contrib(Value)) :- eval_expr(Expr, Value).

group_key(Template, Solution, GroupKey) :-
    findall(Value, ( nth1(Position, Template, plain(_)), nth1(Position, Solution, value(Value)) ),
            GroupKey).

aggregate_args(Template, Group, Args) :-
    findall(Arg,
            ( nth1(Position, Template, TemplateArg),
              template_arg_out(TemplateArg, Position, Group, Arg) ),
            Args).

template_arg_out(plain(_), Position, [Solution | _], Value) :-
    nth1(Position, Solution, value(Value)).
template_arg_out(agg(Kind, _), Position, Group, Value) :-
    findall(Contribution,
            ( member(Solution, Group), nth1(Position, Solution, contrib(Contribution)) ),
            Contributions),
    agg_compute(Kind, Contributions, Value).

agg_compute(count, Contributions, Count) :- length(Contributions, Count).
agg_compute(sum, Contributions, Sum) :- sum_list(Contributions, Sum).
agg_compute(min, Contributions, Min) :- min_list(Contributions, Min).
agg_compute(max, Contributions, Max) :- max_list(Contributions, Max).
agg_compute(json_array, Contributions, Array) :- msort(Contributions, Array).
agg_compute(json_object, Pairs, obj(Object)) :-
    sort(Pairs, Distinct), keysort(Distinct, Object),
    pairs_keys(Object, Keys),
    ( sort(Keys, DistinctKeys), length(Keys, N), length(DistinctKeys, N)
    -> true ; throw(json_object_dup_key(Keys)) ).

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
process_occurrences(Prog, Tick, Frozen, [occ(_, Row) | Rest], Store0, Store, Written) :-
    Prog = prog(Decls, Rules),
    Frozen = frozen(MidLevel, PrevLevel),
    store_rows(Store0, StoreRows),
    append(StoreRows, MidLevel, Visible0), sort(Visible0, Visible),
    append(StoreRows, PrevLevel, PreState0), sort(PreState0, PreState),
    findall(EvaluatedHead,
            ( member((Head <+ Body), Rules),
              copy_term((Head <+ Body), (HeadCopy <+ BodyCopy)),
              trigger_atoms(BodyCopy, Triggers),
              member(Row, Triggers),
              solve(BodyCopy, ctx(Visible, PreState, Tick)),
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
    reverse_preserving(StoreArrived, Store0, StoreArrivedOrdered),
    store_rows(StoreArrivedOrdered, MidBase),
    level_closure(PlainLevel, AggRules, MidBase, Tick, MidLevel),
    ord_subtract(MidLevel, PrevLevel, NewLevelRows),
    stamp_extra(Tick, NewLevelRows, 1000, LevelOccs),
    stamp_extra(Tick, CarryIn, 2000, CarryOccs),
    append([CarryOccs, ArrivalOccs, LevelOccs], Occurrences),
    process_occurrences(Prog, Tick, frozen(MidLevel, PrevLevel), Occurrences,
                        StoreArrivedOrdered, StoreWritten, WrittenRows),
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
    findall(Row, ( member(Row, CarryCandidates), memberchk(+Row, Deltas) ), CarryOut),
    NextTick is Tick + 1.

% absorb_arrivals prepends; restore arrival order for stamped reads.
reverse_preserving(StoreNew, StoreOld, Ordered) :-
    length(StoreOld, Kept),
    length(Suffix, Kept), append(Added, Suffix, StoreNew),
    reverse(Added, InOrder), append(Suffix, InOrder, Ordered).

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
