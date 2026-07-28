% tabling_fixpoint.pl : LAB, parallel tabled evaluator vs the hand-rolled
% fixpoint (plain_fixpoint/5, agg_loop/6 in conformance/level_eval.pl).
%
% Question: can SWI incremental tabling replace level_eval.pl's hand-rolled
% semi-naive loop? This file builds a SECOND level-rule evaluator that
% compiles each fixture's level rules into real dynamic/tabled Prolog
% predicates (namespaced per fixture run), diffs the store snapshot each
% call into assert/retract deltas on the base predicates, and lets SWI's
% incremental tabling engine keep the derived (level-headed) predicates in
% sync. Everything ELSE in a tick (arrivals, edge firing, retention, boundary
% deltas) is reused UNCHANGED from conformance/engine.pl via qualified calls
% (engine:Predicate), so the two evaluators differ ONLY in how the level-rule
% closure is computed. conformance/, plans/, ARCH.pl are read-only; this file
% and its .md are the whole deliverable.
%
% Run: swipl -q -l v6/prolog/labs/tabling_fixpoint.pl -g go -g halt

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(library(ordsets)).
:- use_module(library(pairs)).
:- use_module(library(time)).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

:- prolog_load_context(directory, LabDirectory),
   atomic_list_concat([LabDirectory, '/../conformance/'], ConformanceDirectory),
   atomic_list_concat([ConformanceDirectory, 'body.pl'], BodyPath),
   atomic_list_concat([ConformanceDirectory, 'level_eval.pl'], LevelEvalPath),
   atomic_list_concat([ConformanceDirectory, 'engine.pl'], EnginePath),
   use_module(BodyPath),
   use_module(LevelEvalPath),
   use_module(EnginePath),
   nb_setval(tf_conformance_directory, ConformanceDirectory).

:- multifile user:fixture/5.
:- discontiguous user:fixture/5.

% Every conformance fixtures/*.pl loads, same discovery as go.pl.
load_conformance_fixtures :-
    nb_getval(tf_conformance_directory, ConformanceDirectory),
    atomic_list_concat([ConformanceDirectory, 'fixtures'], FixturesDirectory),
    directory_files(FixturesDirectory, Entries),
    msort(Entries, Ordered),
    forall(( member(Entry, Ordered), sub_atom(Entry, _, 3, 0, '.pl') ),
           ( atomic_list_concat([FixturesDirectory, '/', Entry], Path),
             ensure_loaded(Path) )).

:- load_conformance_fixtures.

% ═══ namespacing: one fresh predicate family per fixture run ═══════════════
% Every DSL rel Ref (Name/Arity) maps to a Prolog predicate tf<RunId>_Name.
% Base (non-level-headed) predicates carry the original arity and are plain
% dynamic-as-incremental fact stores the harness assert/retracts into on
% every level-closure call, diffed against the previous snapshot. Level-
% headed predicates carry arity+1 (a trailing Tick argument, since a rule
% body's now(Tick) needs the live tick number and threading it as an extra
% argument lets distinct ticks occupy distinct table entries without any
% special-case invalidation).

:- dynamic tf_run_counter/1.
:- dynamic tf_rules_installed/1.
:- dynamic tf_base_declared/2.
:- dynamic tf_prev_base/2.
:- dynamic tf_divergence/2.

tf_run_counter(0).

next_run_id(RunId) :-
    retract(tf_run_counter(RunId)),
    NextCounter is RunId + 1,
    assertz(tf_run_counter(NextCounter)).

pred_name(RunId, Name, PredName) :-
    atomic_list_concat([tf, RunId, '_', Name], PredName).

% ═══ multiset diff (Log rels carry duplicate rows; ruling q7 bag multiplicity
% depends on those duplicates surviving into the aggregate bag, so the base
% predicates must gain/lose EXACTLY the row multiplicities that changed) ════

multiset_diff(PreviousRows, CurrentRows, RowsToAdd, RowsToRemove) :-
    remove_common(CurrentRows, PreviousRows, RowsToAdd, RowsToRemove).

remove_common([], RemainingPrevious, [], RemainingPrevious).
remove_common([Row | RestCurrent], PreviousRows, RowsToAdd, RemainingPrevious) :-
    ( select(Row, PreviousRows, PreviousRows1)
    -> remove_common(RestCurrent, PreviousRows1, RowsToAdd, RemainingPrevious)
    ;  RowsToAdd = [Row | RestToAdd],
       remove_common(RestCurrent, PreviousRows, RestToAdd, RemainingPrevious)
    ).

ensure_base_declared(RunId, Ref) :-
    ( tf_base_declared(RunId, Ref)
    -> true
    ;  Ref = Name/Arity,
       pred_name(RunId, Name, PredName),
       dynamic(PredName/Arity as incremental),
       assertz(tf_base_declared(RunId, Ref))
    ).

assert_base_row(RunId, Row) :-
    rel_ref(Row, Ref),
    ensure_base_declared(RunId, Ref),
    Row =.. [Name | Args],
    pred_name(RunId, Name, PredName),
    PredTerm =.. [PredName | Args],
    assertz(PredTerm).

retract_base_row(RunId, Row) :-
    rel_ref(Row, Ref),
    ensure_base_declared(RunId, Ref),
    Row =.. [Name | Args],
    pred_name(RunId, Name, PredName),
    PredTerm =.. [PredName | Args],
    ( retract(PredTerm) -> true ; true ).

apply_base_diff(RunId, RowsToAdd, RowsToRemove) :-
    maplist(assert_base_row(RunId), RowsToAdd),
    maplist(retract_base_row(RunId), RowsToRemove).

% ═══ body translation: a DSL level-rule body becomes a real Prolog goal ════
% Only two shapes change the read path: a plain rel atom (or only/1, which
% reads identically to a plain atom in a level body) becomes a call to the
% namespaced predicate, and not/1 wraps the recursive translation in \+. Every
% other goal shape (:=, is, comparisons, decode, json_each) does not consult
% Visible/PreState in solve/2, so it passes straight through to body:solve/2
% unchanged. departed/1 and pre/1 always fail in a level-rule body (PreState
% is always [] for plain_fixpoint/agg_rule_rows), matched here directly
% instead of by an empty PreState list.

passthrough_goal(_ := _).
passthrough_goal(_ is _).
passthrough_goal(decode(_, _)).
passthrough_goal(json_each(_, _)).
passthrough_goal(Goal) :- body:comparison_goal(Goal).

translate_goal((Left, Right), RunId, LevelHeadedRefs, Tick, (PrologLeft, PrologRight)) :- !,
    translate_goal(Left, RunId, LevelHeadedRefs, Tick, PrologLeft),
    translate_goal(Right, RunId, LevelHeadedRefs, Tick, PrologRight).
translate_goal(not(Inner), RunId, LevelHeadedRefs, Tick, \+ PrologInner) :- !,
    translate_goal(Inner, RunId, LevelHeadedRefs, Tick, PrologInner).
translate_goal(only(Atom), RunId, LevelHeadedRefs, Tick, PrologGoal) :- !,
    translate_goal(Atom, RunId, LevelHeadedRefs, Tick, PrologGoal).
translate_goal(departed(_), _, _, _, fail) :- !.
translate_goal(pre(_), _, _, _, fail) :- !.
translate_goal(now(TickVariable), _, _, Tick, (TickVariable = Tick)) :- !.
translate_goal(true, _, _, _, true) :- !.
translate_goal(Goal, _, _, Tick, body:solve(Goal, ctx([], [], Tick))) :-
    passthrough_goal(Goal), !.
translate_goal(Atom, RunId, LevelHeadedRefs, Tick, PrologGoal) :-
    rel_ref(Atom, Ref),
    Atom =.. [Name | Args],
    pred_name(RunId, Name, PredName),
    ( memberchk(Ref, LevelHeadedRefs)
    -> append(Args, [Tick], FullArgs)
    ;  FullArgs = Args
    ),
    PrologGoal =.. [PredName | FullArgs].

% ═══ installing a fixture's level rules as real predicates (once per run) ══

declare_level_pred(RunId, Name/Arity) :-
    pred_name(RunId, Name, PredName),
    TotalArity is Arity + 1,
    table(PredName/TotalArity as incremental).

% Head values are EXPRESSIONS (body.pl: "named-column rule; evaluate after
% the joins"), so the clause head cannot reuse Head's own argument terms
% directly; it needs fresh result variables and an explicit eval_head/2 step
% (reused from body.pl, unqualified via use_module below) run after the body,
% exactly mirroring plain_fixpoint's solve(Body,...), eval_head(Head, ...).
install_plain_rule(RunId, LevelHeadedRefs, (Head <- Body)) :-
    functor(Head, Name, Arity),
    pred_name(RunId, Name, PredName),
    translate_goal(Body, RunId, LevelHeadedRefs, Tick, TranslatedBody),
    length(ResultArgs, Arity),
    append(ResultArgs, [Tick], HeadArgsFull),
    HeadTerm =.. [PredName | HeadArgsFull],
    assertz((HeadTerm :-
                TranslatedBody,
                body:eval_head(Head, EvaluatedHead),
                EvaluatedHead =.. [_ | ResultArgs])).

% Aggregate heads (q7 bag): the same grouping/aggregation shape as
% level_eval:agg_rule_rows/4, reused verbatim via qualified calls
% (level_eval:head_arg_value/2, group_key/3, aggregate_args/3) so the bag
% semantics, grouping, and json_object dup-key throw are byte-identical to
% the hand-rolled evaluator; only the body solve step differs.
install_agg_rule(RunId, LevelHeadedRefs, (Head <- Body)) :-
    level_eval:aggregate_head(Head, Template, Name/Arity),
    pred_name(RunId, Name, PredName),
    translate_goal(Body, RunId, LevelHeadedRefs, Tick, TranslatedBody),
    length(FreshArgs, Arity),
    append(FreshArgs, [Tick], HeadArgsFull),
    HeadTerm =.. [PredName | HeadArgsFull],
    assertz((HeadTerm :- lab_agg_rows(TranslatedBody, Template, FreshArgs))).

lab_agg_rows(TranslatedBody, Template, Args) :-
    findall(Contribution,
            ( call(TranslatedBody), maplist(level_eval:head_arg_value, Template, Contribution) ),
            Bag),
    Bag \== [],
    findall(GroupKey, ( member(Solution, Bag), level_eval:group_key(Template, Solution, GroupKey) ), Keys0),
    sort(Keys0, GroupKeys),
    member(GroupKey, GroupKeys),
    findall(Solution, ( member(Solution, Bag), level_eval:group_key(Template, Solution, GroupKey) ), Group),
    level_eval:aggregate_args(Template, Group, Args).

% A body referencing a base Ref that never happens to hold a row in THIS
% fixture's schedule would otherwise call an undeclared predicate (Prolog
% raises existence_error for that, where solve/2's member(Atom, Visible)
% would just fail); reusing level_eval:goal_rel_refs/3 (the same body walker
% stratify_level_rules uses) finds every Ref a body reads, positive or
% negated, so every base predicate gets declared once, up front, with zero
% rows if the fixture never happens to populate it.
body_refs((_ <- Body), Refs) :-
    level_eval:goal_rel_refs(Body, PosRefs, NegRefs),
    append(PosRefs, NegRefs, Refs).

install_rules(RunId, PlainLevel, AggRules) :-
    findall(Ref, ( member((Head <- _), PlainLevel), rel_ref(Head, Ref) ), PlainRefs0),
    findall(Ref, ( member((Head <- _), AggRules), level_eval:aggregate_head(Head, _, Ref) ), AggRefs0),
    append(PlainRefs0, AggRefs0, LevelRefs0),
    sort(LevelRefs0, LevelHeadedRefs),
    append(PlainLevel, AggRules, AllLevelRules),
    findall(Ref,
            ( member(Rule, AllLevelRules), body_refs(Rule, RuleRefs), member(Ref, RuleRefs) ),
            AllBodyRefs0),
    sort(AllBodyRefs0, AllBodyRefs),
    subtract(AllBodyRefs, LevelHeadedRefs, BaseRefsNeeded),
    maplist(ensure_base_declared(RunId), BaseRefsNeeded),
    maplist(declare_level_pred(RunId), LevelHeadedRefs),
    maplist(install_plain_rule(RunId, LevelHeadedRefs), PlainLevel),
    maplist(install_agg_rule(RunId, LevelHeadedRefs), AggRules).

ensure_rules_installed(RunId, PlainLevel, AggRules) :-
    ( tf_rules_installed(RunId)
    -> true
    ;  install_rules(RunId, PlainLevel, AggRules),
       assertz(tf_rules_installed(RunId))
    ).

% ═══ collecting the current level (sorted union across all level-headed
% predicates, at the given Tick) ═════════════════════════════════════════════

collect_level(RunId, PlainLevel, AggRules, Tick, Level) :-
    findall(Ref, ( member((Head <- _), PlainLevel), rel_ref(Head, Ref) ), PlainRefs0),
    findall(Ref, ( member((Head <- _), AggRules), level_eval:aggregate_head(Head, _, Ref) ), AggRefs0),
    append(PlainRefs0, AggRefs0, LevelRefs0),
    sort(LevelRefs0, LevelHeadedRefs),
    findall(Row, level_headed_row(RunId, LevelHeadedRefs, Tick, Row), Rows0),
    sort(Rows0, Level).

level_headed_row(RunId, LevelHeadedRefs, Tick, Row) :-
    member(Name/Arity, LevelHeadedRefs),
    pred_name(RunId, Name, PredName),
    length(Args, Arity),
    append(Args, [Tick], FullArgs),
    CallTerm =.. [PredName | FullArgs],
    call(CallTerm),
    Row =.. [Name | Args].

% ═══ the tabled level closure, drop-in for level_eval:level_closure/5 ══════
% Base is the full store-rows snapshot engine.pl always passes (never
% includes level rows, per the one-rel-one-rule-kind law); diffing it against
% the last snapshot this RunId saw turns the snapshot into an assert/retract
% delta on the base predicates, and incremental tabling keeps every
% level-headed predicate's answer set in sync from there.

tabled_level_closure(RunId, PlainLevel, AggRules, Base, Tick, Level) :-
    ensure_rules_installed(RunId, PlainLevel, AggRules),
    ( retract(tf_prev_base(RunId, PreviousBase)) -> true ; PreviousBase = [] ),
    multiset_diff(PreviousBase, Base, RowsToAdd, RowsToRemove),
    apply_base_diff(RunId, RowsToAdd, RowsToRemove),
    assertz(tf_prev_base(RunId, Base)),
    collect_level(RunId, PlainLevel, AggRules, Tick, Level).

% ═══ the tick, mirrored from engine:tick/7 with ONLY the level_closure calls
% swapped; every other step (arrivals, edge firing, retention, boundary
% deltas, carry-out) is the SAME code, called qualified since engine.pl does
% not export these internals ═════════════════════════════════════════════════

tick_tabled(RunId, Prog, state(Tick, Store0, PrevLevel, PrevAll), CarryIn, OutsideArrivals,
            state(NextTick, Store, Level, NextAll), CarryOut, Deltas) :-
    Prog = prog(_, Rules),
    level_eval:split_rules(Rules, AggRules, PlainLevel, _),
    engine:absorb_arrivals(Prog, Tick, OutsideArrivals, Store0, 1, StoreArrived, _, ArrivalOccs),
    engine:store_rows(StoreArrived, MidBase),
    tabled_level_closure(RunId, PlainLevel, AggRules, MidBase, Tick, MidLevel),
    ord_subtract(MidLevel, PrevLevel, NewLevelRows),
    engine:stamp_extra(Tick, NewLevelRows, 1000, LevelOccs),
    engine:stamp_extra(Tick, CarryIn, 2000, CarryOccs),
    append([CarryOccs, ArrivalOccs, LevelOccs], Occurrences),
    engine:process_occurrences(Prog, Tick, frozen(MidLevel, PrevLevel), Occurrences,
                                StoreArrived, StoreWritten, WrittenRows),
    engine:apply_retention(Prog, StoreWritten, Store),
    engine:store_rows(Store, FinalBase),
    tabled_level_closure(RunId, PlainLevel, AggRules, FinalBase, Tick, Level),
    ord_subtract(Level, MidLevel, PostWriteLevelRows),
    append(FinalBase, Level, NextAll0), sort(NextAll0, NextAll),
    engine:boundary_deltas(Prog, Store0, Store, PrevAll, NextAll, Deltas),
    append(WrittenRows, PostWriteLevelRows, CarryCandidates0),
    engine:dedupe_keep_order(CarryCandidates0, CarryCandidates),
    findall(Row, ( member(Row, CarryCandidates), memberchk(+Row, Deltas) ), ArrivalCarry),
    engine:listened_departure_refs(Rules, DepartureRefs),
    findall(dep(Row),
            ( member(-Row, Deltas), rel_ref(Row, DepRef), memberchk(DepRef, DepartureRefs) ),
            DepartureCarry),
    append(ArrivalCarry, DepartureCarry, CarryOut),
    NextTick is Tick + 1.

run_ticks_tabled(_, _, state(_, Store, Level, _), [], [], _, FinalAll, []) :- !,
    engine:store_rows(Store, Rows),
    append(Rows, Level, FinalAll0), msort(FinalAll0, FinalAll).
run_ticks_tabled(RunId, Prog, State, Carry, [Arrivals | Schedule], Drains, FinalAll, [Deltas | More]) :- !,
    tick_tabled(RunId, Prog, State, Carry, Arrivals, NextState, NextCarry, Deltas),
    run_ticks_tabled(RunId, Prog, NextState, NextCarry, Schedule, Drains, FinalAll, More).
run_ticks_tabled(RunId, Prog, State, Carry, [], Drains, FinalAll, [Deltas | More]) :-
    Carry \== [],
    engine:drain_cap(Cap),
    ( Drains >= Cap -> throw(drain_overflow(Cap)) ; true ),
    NextDrains is Drains + 1,
    tick_tabled(RunId, Prog, State, Carry, [], NextState, NextCarry, Deltas),
    run_ticks_tabled(RunId, Prog, NextState, NextCarry, [], NextDrains, FinalAll, More).

run_program_tabled(RunId, Prog, Initial, Schedule, FinalAll, DeltaTicks) :-
    engine:check_program(Prog),
    engine:seed_store(Prog, Initial, Store0),
    Prog = prog(_, Rules),
    level_eval:split_rules(Rules, AggRules, PlainLevel, _),
    engine:store_rows(Store0, BaseRows),
    tabled_level_closure(RunId, PlainLevel, AggRules, BaseRows, 0, Level0),
    append(BaseRows, Level0, All0), sort(All0, PrevAll),
    run_ticks_tabled(RunId, Prog, state(1, Store0, Level0, PrevAll), [], Schedule, 0, FinalAll, DeltaTicks).

run_fixture_tabled(Prog, Initial, Schedule, FinalAll, DeltaTicks) :-
    next_run_id(RunId),
    run_program_tabled(RunId, Prog, Initial, Schedule, FinalAll, DeltaTicks).

% ═══ comparison harness ═════════════════════════════════════════════════════

catch_outcome(Goal, Outcome) :-
    catch((call(Goal), Outcome = ok), Caught, Outcome = threw(Caught)).

run_reference(Prog, Initial, Schedule, Outcome, FinalAll, DeltaTicks) :-
    catch_outcome(engine:run_program(Prog, Initial, Schedule, FinalAll, DeltaTicks), Outcome).

run_tabled(Prog, Initial, Schedule, Outcome, FinalAll, DeltaTicks) :-
    catch_outcome(run_fixture_tabled(Prog, Initial, Schedule, FinalAll, DeltaTicks), Outcome).

record_divergence(Name, Comparison) :- assertz(tf_divergence(Name, Comparison)).

compare_fixture(Name) :-
    user:fixture(Name, Prog, Initial, Schedule, Expectations),
    ( memberchk(throws(Expected), Expectations)
    -> compare_throws(Name, Prog, Initial, Schedule, Expected)
    ;  compare_full(Name, Prog, Initial, Schedule)
    ).

compare_throws(Name, Prog, Initial, Schedule, Expected) :-
    run_reference(Prog, Initial, Schedule, ReferenceOutcome, _, _),
    run_tabled(Prog, Initial, Schedule, TabledOutcome, _, _),
    ( ReferenceOutcome == threw(Expected), TabledOutcome == threw(Expected)
    -> format("PASS ~w throws~n", [Name])
    ;  format("MISMATCH ~w throws~n  expected:  ~q~n  reference: ~q~n  tabled:    ~q~n",
              [Name, Expected, ReferenceOutcome, TabledOutcome]),
       record_divergence(Name, throws)
    ).

compare_full(Name, Prog, Initial, Schedule) :-
    run_reference(Prog, Initial, Schedule, ReferenceOutcome, ReferenceFinal, ReferenceDeltas),
    run_tabled(Prog, Initial, Schedule, TabledOutcome, TabledFinal, TabledDeltas),
    ( ReferenceOutcome == ok, TabledOutcome == ok
    -> compare_final(Name, ReferenceFinal, TabledFinal),
       compare_deltas(Name, ReferenceDeltas, TabledDeltas),
       compare_ticks(Name, ReferenceDeltas, TabledDeltas)
    ;  format("MISMATCH ~w outcome~n  reference: ~q~n  tabled:    ~q~n",
              [Name, ReferenceOutcome, TabledOutcome]),
       record_divergence(Name, outcome)
    ).

compare_final(Name, ReferenceFinal, TabledFinal) :-
    ( ReferenceFinal == TabledFinal
    -> format("PASS ~w final~n", [Name])
    ;  format("MISMATCH ~w final~n  reference rows: ~q~n  tabled rows:    ~q~n",
              [Name, ReferenceFinal, TabledFinal]),
       record_divergence(Name, final)
    ).

compare_deltas(Name, ReferenceDeltas, TabledDeltas) :-
    ( ReferenceDeltas == TabledDeltas
    -> format("PASS ~w deltas~n", [Name])
    ;  format("MISMATCH ~w deltas~n  reference deltas: ~q~n  tabled deltas:    ~q~n",
              [Name, ReferenceDeltas, TabledDeltas]),
       record_divergence(Name, deltas)
    ).

compare_ticks(Name, ReferenceDeltas, TabledDeltas) :-
    length(ReferenceDeltas, ReferenceTickCount),
    length(TabledDeltas, TabledTickCount),
    ( ReferenceTickCount == TabledTickCount
    -> format("PASS ~w ticks~n", [Name])
    ;  format("MISMATCH ~w ticks~n  reference: ~q  tabled: ~q~n",
              [Name, ReferenceTickCount, TabledTickCount]),
       record_divergence(Name, ticks)
    ).

% ═══ retraction-heavy stress fixtures (this lab's own, not in conformance/):
% keyed replace churn, a pure-retraction tick, and a departure-triggered
% chain, since incremental tabling's retraction path is its known hard case
% and the engine itself had a pure-retraction-tick defect historically ═════

fixture(stress_keyed_replace_churn,
  prog([ kind(post/2, log), keep(post/2, all),
         keyed(latest_post/2, [1]) ],
       [ (latest_post(Author, Body) <+ post(Author, Body)),
         (loud_author(Author) <- latest_post(Author, Body), Body == shout) ]),
  [],
  [ [ +post(mira, hello) ],
    [ +post(mira, shout) ],
    [ +post(mira, quiet) ],
    [ +post(mira, shout) ],
    [ +post(mira, shout) ] ],
  [ deltas(latest_post/2,
           [ [ +latest_post(mira, hello) ],
             [ -latest_post(mira, hello), +latest_post(mira, shout) ],
             [ -latest_post(mira, shout), +latest_post(mira, quiet) ],
             [ -latest_post(mira, quiet), +latest_post(mira, shout) ],
             [],
             [] ]),
    deltas(loud_author/1,
           [ [], [ +loud_author(mira) ], [ -loud_author(mira) ], [ +loud_author(mira) ], [], [] ]),
    final(latest_post/2, [ latest_post(mira, shout) ]),
    final(loud_author/1, [ loud_author(mira) ]) ]).

% A pure-retraction tick: the only arrival this tick is a -Row on a Set
% source rel, no +Row at all, and a level rule reading it must retract its
% derived row with nothing else firing (the historical pure-retraction-tick
% defect: reverse_preserving deletion, chat_log 20260727.3).
fixture(stress_pure_retraction_tick,
  prog([ kind(candidate/1, set) ],
       [ (eligible(Name) <- candidate(Name)) ]),
  [ candidate(orion), candidate(vela) ],
  [ [ -candidate(orion) ],
    [] ],
  [ deltas(eligible/1, [ [ -eligible(orion) ], [] ]),
    final(eligible/1, [ eligible(vela) ]),
    final(candidate/1, [ candidate(vela) ]) ]).

% Departure rule chain: a departure(Row) trigger on one keyed rel writes a
% Log rel, which a level rule then aggregates; the departure carries into
% T+1 as its own occurrence (ruling r4), so the count only updates one tick
% after the retraction that caused it.
fixture(stress_departure_chain,
  prog([ kind(assign/2, log), keep(assign/2, all),
         keyed(owner/2, [1]),
         kind(vacated/1, log), keep(vacated/1, all) ],
       [ (owner(Seat, Person) <+ assign(Seat, Person)),
         (vacated(Seat) <+ only(departed(owner(Seat, _)))),
         (vacancy_count(count(Seat)) <- vacated(Seat)) ]),
  [ owner(seat1, ren) ],
  [ [ +assign(seat1, kai) ],
    [] ],
  [ deltas(vacated/1, [ [], [ +vacated(seat1) ] ]),
    deltas(vacancy_count/1, [ [], [ +vacancy_count(1) ] ]),
    final(owner/2, [ owner(seat1, kai) ]),
    final(vacated/1, [ vacated(seat1) ]),
    final(vacancy_count/1, [ vacancy_count(1) ]) ]).

% ═══ microbench: the 3 heaviest fixtures by (rules + initial rows + schedule
% rows) as an inference-volume proxy, timed both ways via time/1, reporting
% inference counts rather than wall-clock feelings ══════════════════════════

fixture_weight(Name, Weight) :-
    user:fixture(Name, prog(_, Rules), Initial, Schedule, _),
    length(Rules, RuleCount),
    length(Initial, InitialCount),
    findall(RowCount, ( member(Arrivals, Schedule), length(Arrivals, RowCount) ), ScheduleCounts),
    sum_list(ScheduleCounts, ScheduleTotal),
    Weight is RuleCount + InitialCount + ScheduleTotal.

heaviest_fixtures(TopNames) :-
    findall(Weight-Name, fixture_weight(Name, Weight), Weighted0),
    sort(0, @>=, Weighted0, Weighted),
    pairs_values(Weighted, Sorted),
    length(TopNames, 3),
    append(TopNames, _, Sorted).

bench_one(Name) :-
    user:fixture(Name, Prog, Initial, Schedule, _),
    format("~nBENCH ~w reference~n", [Name]),
    statistics(inferences, ReferenceBefore),
    time(ignore(engine:run_program(Prog, Initial, Schedule, _, _))),
    statistics(inferences, ReferenceAfter),
    ReferenceInferences is ReferenceAfter - ReferenceBefore,
    format("BENCH ~w reference inferences=~w~n", [Name, ReferenceInferences]),
    format("BENCH ~w tabled~n", [Name]),
    statistics(inferences, TabledBefore),
    time(ignore(run_fixture_tabled(Prog, Initial, Schedule, _, _))),
    statistics(inferences, TabledAfter),
    TabledInferences is TabledAfter - TabledBefore,
    format("BENCH ~w tabled inferences=~w~n", [Name, TabledInferences]).

run_microbench :-
    heaviest_fixtures(TopNames),
    format("~n=== microbench: 3 heaviest fixtures by rule+row weight ===~n", []),
    maplist(bench_one, TopNames).

% ═══ the stratified-negation tripwire (header's hard constraint) ══════════
% None of the 100 corpus fixtures contains a genuine negation cycle (the DSL
% requires stratifiability; check_program never sees this case, and
% stratify_level_rules's own not_stratified throw is untested by any
% promoted fixture). The header names this a tripwire specifically because
% skipping stratify_level_rules/relax_strata (as tabled_level_closure does,
% relying on SWI's own resolution instead) means the tabled evaluator has NO
% code path that rejects a cyclic program the way the hand-rolled one does.
% This probes that directly with the textbook non-stratifiable pair
% p <- not(q), q <- not(p): the hand-rolled evaluator's relax_strata loop
% cannot lower either side without exceeding Cap and throws not_stratified;
% plain incremental tabling has no such cap and just resolves \+ under
% whatever call order the SLG forest reaches first.

stratification_tripwire_program(prog([], [ (p <- not(q)), (q <- not(p)) ])).

probe_stratification_tripwire :-
    stratification_tripwire_program(Prog),
    run_reference(Prog, [], [], ReferenceOutcome, _, _),
    ( catch(call_with_time_limit(5, run_tabled(Prog, [], [], TabledOutcome, _, _)),
            time_limit_exceeded, TabledOutcome = timed_out(5))
    -> true
    ;  TabledOutcome = failed_to_run
    ),
    format("~nTRIPWIRE stratified_negation_cycle~n  reference (hand-rolled): ~q~n  tabled (incremental):    ~q~n",
           [ReferenceOutcome, TabledOutcome]),
    ( ReferenceOutcome == TabledOutcome
    -> format("  tabling reproduces the not_stratified rejection~n", [])
    ;  format("  DIVERGES: tabling accepts and answers a program the hand-rolled evaluator refuses~n", []),
       record_divergence(stratified_negation_cycle, not_stratified_rejection)
    ).

% ═══ entry point ════════════════════════════════════════════════════════════

go :-
    findall(Name, user:fixture(Name, _, _, _, _), FixtureNames),
    maplist(compare_fixture, FixtureNames),
    probe_stratification_tripwire,
    run_microbench,
    length(FixtureNames, TotalFixtures),
    findall(Name-Comparison, tf_divergence(Name, Comparison), Divergences),
    length(Divergences, DivergenceCount),
    format("~n=== SUMMARY: ~w corpus fixtures, ~w total divergences (corpus + tripwire) ===~n",
           [TotalFixtures, DivergenceCount]),
    ( DivergenceCount == 0
    -> format("VERDICT: SHRINKS (no divergence across the corpus or the tripwire)~n", [])
    ;  format("VERDICT: SHIFTS SEMANTICS (~w divergences, see MISMATCH/DIVERGES blocks above)~n",
              [DivergenceCount]),
       forall(member(Name-Comparison, Divergences),
              format("  diverged: ~w (~w)~n", [Name, Comparison]))
    ).
