:- module(dl7_evaluator,
          [ derive_aggregate_rows/4,
            evaluate/4,
            evaluate_stratified/5,
            stratify_rules/3,
            validate_functional_rows/3
          ]).

:- use_module(library(error), [must_be/2]).
:- use_module(library(assoc), [get_assoc/3, list_to_assoc/2]).
:- use_module(library(gensym), [gensym/2]).
:- use_module(library(lists), [max_list/2]).
:- use_module(library(ordsets), [ord_subtract/3, ord_union/3]).
:- use_module(library(pairs), [group_pairs_by_key/2]).
:- use_module(library(ugraphs),
              [ neighbors/3,
                transitive_closure/2,
                vertices_edges_to_ugraph/3
              ]).

:- dynamic evaluation_rule/4.
:- dynamic evaluation_seed/3.
:- dynamic evaluation_lower/3.
:- dynamic evaluation_request/2.
:- dynamic evaluation_recursive/2.
:- dynamic native_relation/4.
:- dynamic cached_recursive_relations/2.

:- table proves/2.

%% evaluate(+Rules, +Seeds, -Closure, -Diagnostics) is det.
%
% Close one ground stratified Datalog program. Compiler and runtime callers use
% the same entry point and checked-goal representation. Each stratum receives
% an immutable completed-lower snapshot and its own cleanup-scoped clauses and
% SLG table.
evaluate(Rules, Seeds, Closure, Diagnostics) :-
    must_be(ground, Rules),
    must_be(ground, Seeds),
    stratify_rules(Rules, Strata, StrataDiagnostics),
    evaluate_after_stratify(StrataDiagnostics, Strata, Rules, Seeds,
                            Closure, Diagnostics).

%% evaluate_stratified(+Rules, +Seeds, +Strata,
%%                     -Closure, -Diagnostics) is det.
%
% Evaluate rules whose checker-owned strata are already available. Compiler
% rounds use this entry point so rule graph closure is computed once.
evaluate_stratified(Rules, Seeds, Strata, Closure, Diagnostics) :-
    must_be(ground, Rules),
    must_be(ground, Seeds),
    must_be(ground, Strata),
    evaluate_after_stratify([], Strata, Rules, Seeds,
                            Closure, Diagnostics).

evaluate_after_stratify([], Strata, Rules, Seeds, Closure, Diagnostics) :-
    !,
    max_stratum(Strata, MaxStratum),
    evaluation_stratum_plans(
        MaxStratum, Strata, Rules, Seeds, StratumPlans),
    gensym(dl7_evaluation_, EvaluationId),
    cached_recursive_relation_set(Rules, RecursiveRelations),
    setup_call_cleanup(
        install_recursive_relations(RecursiveRelations, EvaluationId),
        evaluate_strata(StratumPlans, EvaluationId, [], [],
                        Closure, Diagnostics),
        clear_evaluation(EvaluationId)).
evaluate_after_stratify(Diagnostics, _, _, _, [], Diagnostics).

evaluation_stratum_plans(MaxStratum, Strata, Rules, Seeds, Plans) :-
    maplist(rule_stratum_pair(Strata), Rules, RulePairs0),
    keysort(RulePairs0, RulePairs),
    group_pairs_by_key(RulePairs, RulesByLevel),
    maplist(seed_stratum_pair(Strata), Seeds, SeedPairs0),
    keysort(SeedPairs0, SeedPairs),
    group_pairs_by_key(SeedPairs, SeedsByLevel),
    stratum_plans(0, MaxStratum, RulesByLevel, SeedsByLevel, Plans).

rule_stratum_pair(Strata, Rule, Level-Rule) :-
    rule_at_level(Strata, Level, Rule).

seed_stratum_pair(Strata, Seed, Level-Seed) :-
    seed_at_level(Strata, Level, Seed).

stratum_plans(Level, MaxStratum, _, _, []) :-
    Level > MaxStratum,
    !.
stratum_plans(Level, MaxStratum, RulesByLevel, SeedsByLevel,
              [stratum_plan(PlainRules, AggregateRules, Seeds) | Plans]) :-
    grouped_level_rows(Level, RulesByLevel, Rules),
    include(aggregate_rule, Rules, AggregateRules),
    exclude(aggregate_rule, Rules, PlainRules),
    grouped_level_rows(Level, SeedsByLevel, Seeds),
    NextLevel is Level + 1,
    stratum_plans(NextLevel, MaxStratum, RulesByLevel, SeedsByLevel, Plans).

grouped_level_rows(Level, Groups, Rows) :-
    (   memberchk(Level-Rows0, Groups)
    ->  Rows = Rows0
    ;   Rows = []
    ).

cached_recursive_relation_set(Rules, RecursiveRelations) :-
    (   with_mutex(
            dl7_evaluator_cache,
            cached_recursive_relations(Rules, RecursiveRelations))
    ->  true
    ;   recursive_relations(Rules, RecursiveRelations0),
        with_mutex(
            dl7_evaluator_cache,
            store_recursive_relations(Rules, RecursiveRelations0)),
        RecursiveRelations = RecursiveRelations0
    ).

store_recursive_relations(Rules, RecursiveRelations) :-
    (   cached_recursive_relations(Rules, _)
    ->  true
    ;   assertz(cached_recursive_relations(Rules, RecursiveRelations))
    ).

recursive_relations(Rules, RecursiveRelations) :-
    rule_dependencies(Rules, Dependencies),
    rule_relations(Rules, Relations),
    findall(Head-Body,
            member(dependency(Head, Body, positive, 0, _), Dependencies),
            PositiveEdges),
    vertices_edges_to_ugraph(Relations, PositiveEdges, Graph),
    transitive_closure(Graph, Closure),
    findall(Relation,
            ( member(Relation, Relations),
              neighbors(Relation, Closure, Reachable),
              memberchk(Relation, Reachable)
            ),
            RecursiveRelations).

install_recursive_relations([], _).
install_recursive_relations([Relation | Relations], EvaluationId) :-
    assertz(evaluation_recursive(EvaluationId, Relation)),
    install_recursive_relations(Relations, EvaluationId).

max_stratum([], 0).
max_stratum(Strata, MaxStratum) :-
    findall(Level, member(stratum(_, Level), Strata), Levels),
    max_list(Levels, MaxStratum).

evaluate_strata([], _, Closure, _, Closure, []).
evaluate_strata(
                [stratum_plan(PlainRules, AggregateRules, CurrentSeeds)
                 | StratumPlans], EvaluationId,
                LowerRows, InstalledLowerRows,
                Closure, Diagnostics) :-
    derive_aggregate_rule_rows(LowerRows, AggregateRules,
                               AggregateSeeds, AggregateDiagnostics),
    evaluate_stratum_after_aggregates(
        AggregateDiagnostics, AggregateSeeds,
        StratumPlans, EvaluationId, LowerRows, InstalledLowerRows,
        PlainRules, CurrentSeeds, Closure, Diagnostics).

evaluate_stratum_after_aggregates(
    [], AggregateSeeds,
    StratumPlans, EvaluationId, LowerRows, InstalledLowerRows,
    PlainRules, CurrentSeeds, Closure, Diagnostics) :-
    !,
    append(CurrentSeeds, AggregateSeeds, Seeds0),
    sort(Seeds0, StratumSeeds),
    ord_subtract(LowerRows, InstalledLowerRows, NewLowerRows),
    install_rules(PlainRules, EvaluationId),
    install_seeds(StratumSeeds, EvaluationId),
    install_lower_rows(NewLowerRows, EvaluationId),
    current_stratum_relations(PlainRules, StratumSeeds, Relations),
    collect_native_closure(EvaluationId, Relations, LowerRows, CompletedRows),
    verify_native_closure(EvaluationId, CompletedRows),
    evaluate_strata(StratumPlans, EvaluationId,
                    CompletedRows, LowerRows,
                    Closure, Diagnostics).
evaluate_stratum_after_aggregates(
    Diagnostics, _, _, _, _, _, _, _, _,
    [], Diagnostics).

rule_at_level(Strata, Level, rule(call(Relation, _), _)) :-
    memberchk(stratum(Relation, Level), Strata).

seed_at_level(Strata, Level, call(Relation, _)) :-
    relation_level(Strata, Relation, Level).

relation_level(Strata, Relation, Level) :-
    (   memberchk(stratum(Relation, DerivedLevel), Strata)
    ->  Level = DerivedLevel
    ;   Level = 0
    ).

aggregate_rule(rule(call(_, Arguments), _)) :-
    memberchk(aggregate(count, _), Arguments).

derive_aggregate_rule_rows(_, [], [], []).
derive_aggregate_rule_rows(CompletedRows, [Rule | Rules], Rows, Diagnostics) :-
    derive_aggregate_rows(CompletedRows, Rule, OwnRows, OwnDiagnostics),
    derive_aggregate_rule_rows(CompletedRows, Rules,
                               RestRows, RestDiagnostics),
    append(OwnRows, RestRows, Rows0),
    sort(Rows0, Rows),
    append(OwnDiagnostics, RestDiagnostics, Diagnostics0),
    sort(Diagnostics0, Diagnostics).

%% derive_aggregate_rows(+CompletedRows, +Rule, -Rows, -Diagnostics) is det.
%
% Enumerate complete body proofs against one immutable lower-row snapshot.
% Plain head positions form the group key. Every proof contributes one bag
% entry, including equal count expressions reached through distinct bindings.
derive_aggregate_rows(CompletedRows, Rule, Rows, Diagnostics) :-
    must_be(ground, CompletedRows),
    must_be(ground, Rule),
    Rule = rule(call(_, HeadArguments), _),
    aggregate_arguments(HeadArguments, Aggregates),
    length(Aggregates, AggregateCount),
    derive_checked_aggregate(AggregateCount, CompletedRows, Rule,
                             Rows, Diagnostics).

derive_checked_aggregate(1, CompletedRows, Rule, Rows, Diagnostics) :-
    !,
    findall(Head,
            aggregate_rule_proof(CompletedRows, Rule, Head),
            ProofHeads),
    (   ground(ProofHeads)
    ->  aggregate_proofs(ProofHeads, Proofs0),
        msort(Proofs0, Proofs),
        grouped_aggregate_rows(Proofs, Rows0),
        sort(Rows0, Rows),
        Diagnostics = []
    ;   Rows = [],
        Diagnostics = [diagnostic(evaluate, none,
                                  non_ground_aggregate_proof)]
    ).
derive_checked_aggregate(AggregateCount, _, _, [],
                         [diagnostic(evaluate, none,
                                     malformed_aggregate_head(
                                         AggregateCount))]).

aggregate_rule_proof(CompletedRows, Rule, Head) :-
    instantiate_rule(Rule, Head, Body),
    completed_body_holds(Body, CompletedRows).

completed_body_holds([], _).
completed_body_holds([checked_goal(positive, Call) | Goals], Rows) :-
    member(Call, Rows),
    completed_body_holds(Goals, Rows).
completed_body_holds([checked_goal(negative, Call) | Goals], Rows) :-
    ground(Call),
    \+ memberchk(Call, Rows),
    completed_body_holds(Goals, Rows).

aggregate_proofs([], []).
aggregate_proofs([Head | Heads], [proof(Key, Head) | Proofs]) :-
    aggregate_group_key(Head, Key),
    aggregate_proofs(Heads, Proofs).

aggregate_group_key(call(Relation, Arguments), group(Relation, Plain)) :-
    exclude(count_aggregate, Arguments, Plain).

grouped_aggregate_rows([], []).
grouped_aggregate_rows([proof(Key, Head) | Proofs], [Row | Rows]) :-
    take_aggregate_group(Proofs, Key, 1, Count, Rest),
    aggregate_output_row(Head, Count, Row),
    grouped_aggregate_rows(Rest, Rows).

take_aggregate_group([proof(Key, _) | Proofs], Key, Count0, Count, Rest) :-
    !,
    Count1 is Count0 + 1,
    take_aggregate_group(Proofs, Key, Count1, Count, Rest).
take_aggregate_group(Rest, _, Count, Count, Rest).

aggregate_output_row(call(Relation, Arguments0), Count,
                     call(Relation, Arguments)) :-
    maplist(aggregate_output_argument(Count), Arguments0, Arguments).

aggregate_output_argument(Count, aggregate(count, _), const(Count)) :- !.
aggregate_output_argument(_, Argument, Argument).

%% validate_functional_rows(+Relations, +Rows, -Diagnostics) is det.
%
% Check every declared zero-based functional key against a completed closure.
% Complete-row set identity is already enforced by evaluate/4 sorting its
% output. A relation with no declared keys therefore needs no additional
% validation.
validate_functional_rows(Relations, Rows, Diagnostics) :-
    must_be(ground, Relations),
    must_be(ground, Rows),
    sort(Rows, SortedRows),
    rows_by_relation(SortedRows, RowGroups),
    relation_key_diagnostics(Relations, RowGroups, Diagnostics0),
    sort(Diagnostics0, Diagnostics).

rows_by_relation(Rows, RowGroups) :-
    maplist(row_relation_pair, Rows, RowPairs),
    keysort(RowPairs, SortedPairs),
    group_pairs_by_key(SortedPairs, RowGroups).

row_relation_pair(Row, Relation-Row) :-
    Row = call(Relation, _).

relation_key_diagnostics([], _, []).
relation_key_diagnostics([relation(Relation, _, KeySets) | Relations], Groups,
                         Diagnostics) :-
    (   memberchk(Relation-GroupedRows, Groups)
    ->  RelationRows = GroupedRows
    ;   RelationRows = []
    ),
    key_sets_diagnostics(KeySets, Relation, RelationRows, OwnDiagnostics),
    relation_key_diagnostics(Relations, Groups, RestDiagnostics),
    append(OwnDiagnostics, RestDiagnostics, Diagnostics).

key_sets_diagnostics([], _, _, []).
key_sets_diagnostics([Positions | KeySets], Relation, Rows, Diagnostics) :-
    findall(Diagnostic,
            functional_key_conflict(Relation, Positions, Rows, Diagnostic),
            OwnDiagnostics),
    key_sets_diagnostics(KeySets, Relation, Rows, RestDiagnostics),
    append(OwnDiagnostics, RestDiagnostics, Diagnostics).

functional_key_conflict(Relation, Positions, Rows,
                        diagnostic(evaluate, none,
                                   functional_key_conflict(
                                       Relation, Positions, Values,
                                       Left, Right))) :-
    keyed_rows(Rows, Positions, KeyedRows0),
    keysort(KeyedRows0, KeyedRows),
    group_pairs_by_key(KeyedRows, KeyGroups),
    member(Values-ConflictingRows, KeyGroups),
    ordered_row_pair(ConflictingRows, Left, Right).

keyed_rows([], _, []).
keyed_rows([Row | Rows], Positions, [Values-Row | KeyedRows]) :-
    key_values(Row, Positions, Values),
    keyed_rows(Rows, Positions, KeyedRows).

ordered_row_pair([Left | Rows], Left, Right) :- member(Right, Rows).
ordered_row_pair([_ | Rows], Left, Right) :- ordered_row_pair(Rows, Left, Right).

key_values(call(_, Arguments), Positions, Values) :-
    maplist(argument_at(Arguments), Positions, Values).

argument_at(Arguments, Position, Value) :- nth0(Position, Arguments, Value).

%% stratify_rules(+Rules, -DerivedStrata, -Diagnostics) is det.
%
% Derive the least relation stratum satisfying every checked dependency.
% Positive reads have gap zero and negative reads have gap one. A strict edge
% on a dependency cycle is diagnosed before evaluator state is installed.
stratify_rules(Rules, DerivedStrata, Diagnostics) :-
    must_be(ground, Rules),
    rule_dependencies(Rules, Dependencies),
    rule_relations(Rules, Relations),
    strict_cycle_diagnostics(Relations, Dependencies, CycleDiagnostics),
    (   CycleDiagnostics == []
    ->  initial_levels(Relations, InitialLevels),
        relax_to_fixpoint(Dependencies, InitialLevels, Levels),
        derived_relations(Rules, DerivedRelations),
        strata_for_relations(DerivedRelations, Levels, DerivedStrata),
        Diagnostics = []
    ;   DerivedStrata = [],
        Diagnostics = CycleDiagnostics
    ).

rule_dependencies([], []).
rule_dependencies([rule(call(HeadRelation, HeadArguments), Goals) | Rules],
                  Dependencies) :-
    aggregate_arguments(HeadArguments, Aggregates),
    aggregate_dependency_mode(Aggregates, AggregateMode),
    goal_dependencies(Goals, HeadRelation, AggregateMode, OwnDependencies),
    rule_dependencies(Rules, RestDependencies),
    append(OwnDependencies, RestDependencies, Dependencies).

goal_dependencies([], _, _, []).
goal_dependencies(
    [checked_goal(Polarity, call(BodyRelation, _)) | Goals], HeadRelation,
    AggregateMode,
    [dependency(HeadRelation, BodyRelation, Polarity, Gap, Cause)
     | Dependencies]) :-
    dependency_gap(AggregateMode, Polarity, Gap, Cause),
    goal_dependencies(Goals, HeadRelation, AggregateMode, Dependencies).

aggregate_arguments(Arguments, Aggregates) :-
    include(count_aggregate, Arguments, Aggregates).

count_aggregate(aggregate(count, _)).

aggregate_dependency_mode([], plain).
aggregate_dependency_mode([_ | _], aggregate).

dependency_gap(aggregate, _, 1, aggregate).
dependency_gap(plain, Polarity, Gap, Polarity) :- polarity_gap(Polarity, Gap).

polarity_gap(positive, 0).
polarity_gap(negative, 1).

rule_relations(Rules, Relations) :-
    findall(Relation,
            ( member(rule(call(HeadRelation, _), Goals), Rules),
              ( Relation = HeadRelation
              ; member(checked_goal(_, call(Relation, _)), Goals)
              )
            ),
            Relations0),
    sort(Relations0, Relations).

derived_relations(Rules, Relations) :-
    findall(Relation,
            member(rule(call(Relation, _), _), Rules),
            Relations0),
    sort(Relations0, Relations).

strict_cycle_diagnostics([], _, []) :- !.
strict_cycle_diagnostics(Relations, Dependencies, Diagnostics) :-
    dependency_edges(Dependencies, Edges),
    vertices_edges_to_ugraph(Relations, Edges, Graph),
    transitive_closure(Graph, Closure),
    findall(strict_edge(Cause, HeadRelation, BodyRelation),
            ( member(dependency(HeadRelation, BodyRelation, _, 1, Cause),
                     Dependencies),
              neighbors(BodyRelation, Closure, Reachable),
              memberchk(HeadRelation, Reachable)
            ),
            StrictEdges0),
    sort(StrictEdges0, StrictEdges),
    strict_edges_diagnostics(StrictEdges, Relations, Closure, Diagnostics).

strict_edges_diagnostics([], _, _, []) :- !.
strict_edges_diagnostics(StrictEdges, Relations, Closure,
                         [diagnostic(stratify, none, CycleDiagnostic)]) :-
    findall(Relation,
            ( member(strict_edge(_, Head, _), StrictEdges),
              member(Relation, Relations),
              mutually_reachable(Head, Relation, Closure)
            ),
            CycleRelations0),
    sort(CycleRelations0, CycleRelations),
    cycle_diagnostic(StrictEdges, CycleRelations, CycleDiagnostic).

cycle_diagnostic(StrictEdges, Relations,
                 aggregate_dependency_cycle(Relations)) :-
    memberchk(strict_edge(aggregate, _, _), StrictEdges),
    !.
cycle_diagnostic(_, Relations, strict_dependency_cycle(Relations)).

mutually_reachable(Relation, Relation, _) :- !.
mutually_reachable(Left, Right, Closure) :-
    neighbors(Left, Closure, LeftReachable),
    memberchk(Right, LeftReachable),
    neighbors(Right, Closure, RightReachable),
    memberchk(Left, RightReachable).

dependency_edges([], []).
dependency_edges([dependency(HeadRelation, BodyRelation, _, _, _)
                  | Dependencies],
                 [HeadRelation-BodyRelation | Edges]) :-
    dependency_edges(Dependencies, Edges).

initial_levels([], []).
initial_levels([Relation | Relations],
               [level(Relation, 0) | Levels]) :-
    initial_levels(Relations, Levels).

relax_to_fixpoint(Dependencies, Levels0, Levels) :-
    dependency_groups(Dependencies, DependencyGroups),
    list_to_assoc(DependencyGroups, DependencyIndex),
    relax_indexed_to_fixpoint(DependencyIndex, Levels0, Levels).

dependency_groups(Dependencies, Groups) :-
    maplist(dependency_head_pair, Dependencies, Pairs0),
    keysort(Pairs0, Pairs),
    group_pairs_by_key(Pairs, Groups).

dependency_head_pair(Dependency, Head-Dependency) :-
    Dependency = dependency(Head, _, _, _, _).

relax_indexed_to_fixpoint(DependencyIndex, Levels0, Levels) :-
    maplist(level_pair, Levels0, LevelPairs),
    list_to_assoc(LevelPairs, LevelIndex),
    relax_levels(Levels0, LevelIndex, DependencyIndex, Levels1),
    (   Levels1 == Levels0
    ->  Levels = Levels1
    ;   relax_indexed_to_fixpoint(DependencyIndex, Levels1, Levels)
    ).

level_pair(level(Relation, Level), Relation-Level).

relax_levels([], _, _, []).
relax_levels([level(Relation, Current) | Levels0], LevelIndex,
             DependencyIndex,
             [level(Relation, Next) | Levels]) :-
    dependency_requirements(
        Relation, DependencyIndex, LevelIndex, Requirements0),
    Requirements = [Current | Requirements0],
    max_list(Requirements, Next),
    relax_levels(Levels0, LevelIndex, DependencyIndex, Levels).

dependency_requirements(Relation, DependencyIndex, LevelIndex,
                        Requirements) :-
    (   get_assoc(Relation, DependencyIndex, Dependencies)
    ->  true
    ;   Dependencies = []
    ),
    findall(Required,
            ( member(dependency(_, BodyRelation, _, Gap, _), Dependencies),
              get_assoc(BodyRelation, LevelIndex, BodyLevel),
              Required is BodyLevel + Gap
            ),
            Requirements).

strata_for_relations([], _, []).
strata_for_relations([Relation | Relations], Levels,
                     [stratum(Relation, Level) | Strata]) :-
    memberchk(level(Relation, Level), Levels),
    strata_for_relations(Relations, Levels, Strata).

install_rules([], _).
install_rules([Rule | Rules], EvaluationId) :-
    instantiate_rule(Rule, Head, Body),
    Head = call(Relation, _),
    install_reference_rule(EvaluationId, Relation, Head, Body),
    install_native_rule(EvaluationId, Head, Body),
    install_rules(Rules, EvaluationId).

install_reference_rule(EvaluationId, Relation, Head, Body) :-
    (   reference_evaluator_enabled
    ->  assertz(evaluation_rule(EvaluationId, Relation, Head, Body))
    ;   true
    ).

install_seeds([], _).
install_seeds([Seed | Seeds], EvaluationId) :-
    Seed = call(Relation, _),
    install_reference_seed(EvaluationId, Relation, Seed),
    install_native_row(EvaluationId, Seed),
    install_seeds(Seeds, EvaluationId).

install_reference_seed(EvaluationId, Relation, Seed) :-
    (   reference_evaluator_enabled
    ->  assertz(evaluation_seed(EvaluationId, Relation, Seed))
    ;   true
    ).

install_lower_rows(Rows, EvaluationId) :-
    (   reference_evaluator_enabled
    ->  install_reference_lower_rows(Rows, EvaluationId)
    ;   true
    ).

install_reference_lower_rows([], _).
install_reference_lower_rows([Row | Rows], EvaluationId) :-
    Row = call(Relation, _),
    assertz(evaluation_lower(EvaluationId, Relation, Row)),
    install_reference_lower_rows(Rows, EvaluationId).

install_native_rule(EvaluationId, Head, Body) :-
    native_call(EvaluationId, Head, NativeHead),
    native_body(EvaluationId, Body, NativeBody),
    assertz((NativeHead :- NativeBody)).

native_body(_, [], true).
native_body(EvaluationId, [Goal | Goals], (NativeGoal, NativeGoals)) :-
    native_goal(EvaluationId, Goal, NativeGoal),
    native_body(EvaluationId, Goals, NativeGoals).

native_goal(EvaluationId, checked_goal(positive, Call), NativeGoal) :-
    native_positive_goal(EvaluationId, Call, NativeGoal).
native_goal(EvaluationId, checked_goal(negative, Call),
            ( ground(Call),
              \+ NativeCall
            )) :-
    native_call(EvaluationId, Call, NativeCall).

native_positive_goal(_, call(ref(kernel(nil)), [const([])]), true) :- !.
native_positive_goal(_, call(ref(kernel(cons)), [Head, Tail, List]),
                     cons_relation(Head, Tail, List)) :-
    !.
native_positive_goal(EvaluationId,
                     call(ref(kernel(intern)),
                          [Constructor, Arguments, Result]),
                     ( ground(Constructor),
                       ground(Arguments),
                       intern_value(Constructor, Arguments, Result),
                       record_evaluation_request(EvaluationId, Request)
                     )) :-
    !,
    Request = call(ref(kernel(intern)),
                   [Constructor, Arguments, Result]).
native_positive_goal(EvaluationId, Call, NativeGoal) :-
    native_call(EvaluationId, Call, NativeGoal).

install_native_row(EvaluationId, Call) :-
    native_call(EvaluationId, Call, NativeCall),
    assertz(NativeCall).

native_call(EvaluationId, call(Relation, Arguments), NativeCall) :-
    length(Arguments, Arity),
    native_relation_identity(EvaluationId, Relation, Arity, Functor),
    compound_name_arguments(NativeCall, Functor, Arguments).

native_relation_identity(EvaluationId, Relation, Arity, Functor) :-
    (   native_relation(EvaluationId, Relation, Arity, ExistingFunctor)
    ->  Functor = ExistingFunctor
    ;   gensym(dl7_native_relation_, Functor),
        assertz(native_relation(EvaluationId, Relation, Arity, Functor)),
        dynamic(Functor/Arity),
        table_recursive_relation(EvaluationId, Relation, Functor, Arity)
    ).

table_recursive_relation(EvaluationId, Relation, Functor, Arity) :-
    (   evaluation_recursive(EvaluationId, Relation)
    ->  table(Functor/Arity)
    ;   true
    ).

collect_closure(EvaluationId, Closure) :-
    findall(Call, proves(EvaluationId, Call), Calls),
    findall(Request, evaluation_request(EvaluationId, Request), Requests),
    append(Calls, Requests, Rows),
    sort(Rows, Closure).

current_stratum_relations(Rules, Seeds, Relations) :-
    findall(Relation,
            ( member(rule(call(Relation, _), _), Rules)
            ; member(call(Relation, _), Seeds)
            ),
            Relations0),
    sort(Relations0, Relations).

collect_native_closure(EvaluationId, Relations, LowerRows, Closure) :-
    findall(call(Relation, Arguments),
            ( member(Relation, Relations),
              native_relation(EvaluationId, Relation, Arity, Functor),
              length(Arguments, Arity),
              compound_name_arguments(NativeCall, Functor, Arguments),
              call(NativeCall)
            ),
            DerivedCalls),
    Calls = [call(ref(kernel(nil)), [const([])]) | DerivedCalls],
    findall(Request, evaluation_request(EvaluationId, Request), Requests),
    append(Calls, Requests, CurrentRows0),
    sort(CurrentRows0, CurrentRows),
    ord_union(LowerRows, CurrentRows, Closure).

verify_native_closure(EvaluationId, NativeRows) :-
    (   reference_evaluator_enabled
    ->  abolish_table_subgoals(dl7_evaluator:proves(EvaluationId, _)),
        collect_closure(EvaluationId, ReferenceRows),
        compare_evaluator_rows(ReferenceRows, NativeRows)
    ;   true
    ).

reference_evaluator_enabled :-
    getenv('DL7_VERIFY_EVALUATOR', '1').

compare_evaluator_rows(Rows, Rows) :- !.
compare_evaluator_rows(ReferenceRows, NativeRows) :-
    ord_subtract(ReferenceRows, NativeRows, Missing),
    ord_subtract(NativeRows, ReferenceRows, Extra),
    throw(error(native_evaluator_mismatch(Missing, Extra), _)).

clear_evaluation(EvaluationId) :-
    findall(Functor/Arity,
            retract(native_relation(EvaluationId, _, Arity, Functor)),
            NativePredicates),
    maplist(clear_native_predicate, NativePredicates),
    abolish_table_subgoals(dl7_evaluator:proves(EvaluationId, _)),
    retractall(evaluation_request(EvaluationId, _)),
    retractall(evaluation_recursive(EvaluationId, _)),
    retractall(evaluation_rule(EvaluationId, _, _, _)),
    retractall(evaluation_seed(EvaluationId, _, _)),
    retractall(evaluation_lower(EvaluationId, _, _)).

clear_native_predicate(Functor/Arity) :-
    functor(Goal, Functor, Arity),
    abolish_table_subgoals(dl7_evaluator:Goal),
    abolish(Functor/Arity).

proves(EvaluationId, Call) :-
    Call = call(Relation, _),
    evaluation_seed(EvaluationId, Relation, Call).
proves(EvaluationId, Call) :-
    Call = call(Relation, _),
    evaluation_lower(EvaluationId, Relation, Call).
proves(_, call(ref(kernel(nil)), [const([])])).
proves(EvaluationId, Head) :-
    Head = call(Relation, _),
    evaluation_rule(EvaluationId, Relation, Head, Body),
    proves_body(Body, EvaluationId).
proves(_, call(ref(kernel(cons)), [Head, Tail, List])) :-
    cons_relation(Head, Tail, List).
proves(EvaluationId,
       call(ref(kernel(intern)), [Constructor, Arguments, Result])) :-
    ground(Constructor),
    ground(Arguments),
    intern_value(Constructor, Arguments, Result),
    Request = call(ref(kernel(intern)),
                   [Constructor, Arguments, Result]),
    record_evaluation_request(EvaluationId, Request).

record_evaluation_request(EvaluationId, Request) :-
    (   evaluation_request(EvaluationId, Request)
    ->  true
    ;   assertz(evaluation_request(EvaluationId, Request))
    ).

proves_body([], _).
proves_body([Goal | Goals], EvaluationId) :-
    satisfy_goal(EvaluationId, Goal),
    proves_body(Goals, EvaluationId).

%% goal_call(+CheckedGoal, -Polarity, -Call) is det.
goal_call(checked_goal(Polarity, Call), Polarity, Call).

%% satisfy_goal(+EvaluationId, +CheckedGoal) is nondet.
satisfy_goal(EvaluationId, Goal) :-
    goal_call(Goal, positive, Call),
    proves(EvaluationId, Call).
satisfy_goal(EvaluationId, Goal) :-
    goal_call(Goal, negative, Call),
    ground(Call),
    Call = call(Relation, _),
    \+ evaluation_lower(EvaluationId, Relation, Call).

%% cons_relation(?Head, ?Tail, ?List) is semidet.
%
% A completed nonempty proper list determines its head and tail. Otherwise a
% ground head and tail construct the list. The empty proper list and improper
% lists have no cons tuple.
cons_relation(Head, Tail, List) :-
    (   ground(List)
    ->  cons_deconstruct(List, Head, Tail)
    ;   ground(Head),
        ground(Tail),
        cons_construct(Head, Tail, List)
    ).

cons_construct(Head, const([]), const([Head])).
cons_construct(Head, const(Tail), const([Head | Tail])) :-
    is_list(Tail).

cons_deconstruct(const([Head]), Head, const([])) :- !.
cons_deconstruct(const([Head | Tail]), Head, const(Tail)) :-
    Tail = [_ | _],
    is_list(Tail).

intern_value(ref(Constructor), const(TaggedArguments),
             ref(application(Constructor, Arguments))) :-
    is_list(TaggedArguments),
    maplist(semantic_argument, TaggedArguments, Arguments).

semantic_argument(ref(Identity), Identity).
semantic_argument(const(Value), Value).

%% instantiate_rule(+Rule, -Head, -Body) is det.
%
% Reified var(Identity) terms share one native SWI variable while one rule is
% being proved. Each later proof receives a fresh identity-to-variable map.
instantiate_rule(rule(Head0, Body0), Head, Body) :-
    instantiate_call(Head0, [], Variables0, Head),
    instantiate_goals(Body0, Variables0, _, Body).

%% instantiate_goals(+Goals0, +Variables0, -Variables, -Goals) is det.
instantiate_goals([], Variables, Variables, []).
instantiate_goals([checked_goal(Polarity, Call0) | Goals0],
                  Variables0, Variables,
                  [checked_goal(Polarity, Call) | Goals]) :-
    instantiate_call(Call0, Variables0, Variables1, Call),
    instantiate_goals(Goals0, Variables1, Variables, Goals).

instantiate_call(call(Relation, Arguments0), Variables0, Variables,
                 call(Relation, Arguments)) :-
    instantiate_arguments(Arguments0, Variables0, Variables, Arguments).

instantiate_arguments([], Variables, Variables, []).
instantiate_arguments([Argument0 | Arguments0], Variables0, Variables,
                      [Argument | Arguments]) :-
    instantiate_argument(Argument0, Variables0, Variables1, Argument),
    instantiate_arguments(Arguments0, Variables1, Variables, Arguments).

instantiate_argument(var(Identity), Variables0, Variables, Variable) :-
    !,
    variable_for_identity(Identity, Variables0, Variables, Variable).
instantiate_argument(aggregate(count, Expression0), Variables0, Variables,
                     aggregate(count, Expression)) :-
    !,
    instantiate_argument(Expression0, Variables0, Variables, Expression).
instantiate_argument(Argument, Variables, Variables, Argument).

variable_for_identity(Identity, Variables0, Variables, Variable) :-
    (   memberchk(Identity-Existing, Variables0)
    ->  Variable = Existing,
        Variables = Variables0
    ;   Variables = [Identity-Variable | Variables0]
    ).
