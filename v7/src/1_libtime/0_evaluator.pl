:- module(dl7_evaluator,
          [ derive_aggregate_rows/4,
            evaluate/4,
            integer_comparison/3,
            stratify_rules/3,
            validate_functional_rows/3
          ]).

:- use_module(library(aggregate), [aggregate_all/3]).
:- use_module(library(assoc),
              [ get_assoc/3,
                list_to_assoc/2,
                put_assoc/4
              ]).
:- use_module(library(error), [must_be/2]).
:- use_module(library(gensym), [gensym/2]).
:- use_module(library(lists), [max_list/2]).
:- use_module(library(ordsets), [ord_union/3]).
:- use_module(library(pairs), [group_pairs_by_key/2]).
:- use_module(library(tableutil), [table_statistics/2]).
:- use_module(library(ugraphs),
              [ neighbors/3,
                transitive_closure/2,
                vertices_edges_to_ugraph/3
              ]).
:- use_module('../2_comptime/1b_compiler_tracer',
              [ compile_scope_memo_lookup/3,
                compile_scope_memo_store/3,
                in_compile_scope/0,
                run_compile_step/4,
                debug_trace_on/0,
                debug_event/2,
                debug_histogram_fields/3,
                profile_scope_on/0,
                profile_occurrence/2
              ]).

:- dynamic evaluation_rule/3.
:- dynamic evaluation_seed/3.
:- dynamic evaluation_lower_index/7.
:- dynamic evaluation_request/2.

:- thread_local debug_worklist_visits/1.
:- thread_local debug_worklist_changes/1.

:- thread_local lower_store_scope/1.
:- thread_local lower_store_installed/2.

:- table proves/2.

%% integer_comparison(?Name, ?PositiveOperator, ?NegativeOperator) is nondet.
%
% The kernel comparison family shares one grounded integer contract. The two
% operators are logical complements, so positive and negative evaluation use
% the same registry without materializing relation rows.
integer_comparison(int_lt, '<', '>=').
integer_comparison(int_le, '=<', '>').
integer_comparison(int_eq, '=:=', '=\\=').
integer_comparison(int_ne, '=\\=', '=:=').
integer_comparison(int_ge, '>=', '<').
integer_comparison(int_gt, '>', '=<').

%% evaluate(+Rules, +Seeds, -Closure, -Diagnostics) is det.
%
% Close one ground stratified Datalog program. Compiler and runtime callers use
% the same entry point and checked-goal representation. Each stratum receives
% an immutable completed-lower snapshot and its own cleanup-scoped clauses and
% SLG table.
evaluate(Rules, Seeds, Closure, Diagnostics) :-
    must_be(ground, Rules),
    must_be(ground, Seeds),
    rule_dependencies(Rules, Dependencies),
    stratify_rules_with_dependencies(
        Rules, Dependencies, Strata, StrataDiagnostics),
    setup_call_cleanup(
        open_lower_store,
        evaluate_after_stratify(
            StrataDiagnostics, Strata, Dependencies, Rules, Seeds,
            Closure, Diagnostics),
        close_lower_store).

%% open_lower_store is det.
%
% Open one evaluate-scope lower-row store for the strata of a single evaluate/4
% call. The immutable completed lower rows are identical terms across strata
% and nested snapshot to snapshot, so the store keeps exactly one clause per
% distinct row for the whole call. Each stratum still installs its own rules,
% seeds, SLG table, and proof identity under its own EvaluationId; only the
% lower-row clauses are shared.
open_lower_store :-
    gensym(dl7_lower_store_, StoreId),
    asserta(lower_store_scope(StoreId)),
    assertz(lower_store_installed(StoreId, [])).

%% close_lower_store is det.
%
% Erase every shared lower-row clause and the scope state. Runs on success,
% failure, and exception through setup_call_cleanup/3 in evaluate/4, so no
% shared row outlives the evaluate call. The most recently opened scope is
% removed first, so a nested evaluate call unwinds its own store.
close_lower_store :-
    (   retract(lower_store_scope(StoreId))
    ->  retractall(evaluation_lower_index(StoreId, _, _, _, _, _, _)),
        retractall(lower_store_installed(StoreId, _))
    ;   true
    ).

%% lower_store_id(+EvaluationId, -StoreId) is det.
%
% Resolve the store that holds this evaluation's lower rows. Inside an
% evaluate/4 scope every stratum reads the one shared store; outside a scope
% the EvaluationId is its own store, preserving the direct install/lookup
% behavior used by callers that manage their own references.
lower_store_id(EvaluationId, StoreId) :-
    (   lower_store_scope(ActiveStore)
    ->  StoreId = ActiveStore
    ;   StoreId = EvaluationId
    ).

evaluate_after_stratify(
    [], Strata, Dependencies, Rules, Seeds, Closure, Diagnostics) :-
    !,
    max_stratum(Strata, MaxStratum),
    evaluate_strata(0, MaxStratum, Strata, Dependencies, Rules, Seeds, [],
                    Closure, Diagnostics).
evaluate_after_stratify(Diagnostics, _, _, _, _, [], Diagnostics).

max_stratum([], 0).
max_stratum(Strata, MaxStratum) :-
    findall(Level, member(stratum(_, Level), Strata), Levels),
    max_list(Levels, MaxStratum).

evaluate_strata(Level, MaxStratum, _, _, _, _, Closure, Closure, []) :-
    Level > MaxStratum,
    !.
evaluate_strata(Level, MaxStratum, Strata, Dependencies, Rules, Seeds, LowerRows,
                Closure, Diagnostics) :-
    include(rule_at_level(Strata, Level), Rules, CurrentRules),
    include(seed_at_level(Strata, Level), Seeds, CurrentSeeds),
    include(aggregate_rule, CurrentRules, AggregateRules),
    demand_cone_rules(
        Strata, Level, Rules, Dependencies, CurrentRules, PlainRules),
    derive_aggregate_rule_rows(LowerRows, AggregateRules,
                               AggregateSeeds, AggregateDiagnostics),
    evaluate_stratum_after_aggregates(
        AggregateDiagnostics, AggregateSeeds,
        Level, MaxStratum, Strata, Dependencies, Rules, Seeds, LowerRows,
        PlainRules, CurrentRules, CurrentSeeds, Closure, Diagnostics).

evaluate_stratum_after_aggregates(
    [], AggregateSeeds,
    Level, MaxStratum, Strata, Dependencies, Rules, Seeds, LowerRows,
    PlainRules, CurrentRules, CurrentSeeds, Closure, Diagnostics) :-
    !,
    append(CurrentSeeds, AggregateSeeds, Seeds0),
    sort(Seeds0, StratumSeeds),
    current_result_relations(CurrentRules, StratumSeeds, ResultRelations),
    gensym(dl7_evaluation_, EvaluationId),
    setup_call_cleanup(
        run_compile_step(
            evaluator, evaluate_install(Level),
            install_evaluation(EvaluationId, PlainRules, StratumSeeds, LowerRows,
                               ClauseReferences),
            evaluate_install_metrics(
                Level, PlainRules, StratumSeeds, LowerRows)),
        run_compile_step(
            evaluator, evaluate_collect(Level),
            collect_closure(EvaluationId, ResultRelations, LowerRows,
                            CompletedRows),
            evaluate_collect_metrics(Level, CompletedRows)),
        run_compile_step(
            evaluator, evaluate_cleanup(Level),
            clear_evaluation(EvaluationId, ClauseReferences),
            evaluate_cleanup_metrics(Level, EvaluationId, ClauseReferences))),
    NextLevel is Level + 1,
    evaluate_strata(NextLevel, MaxStratum, Strata, Dependencies, Rules, Seeds,
                    CompletedRows, Closure, Diagnostics).
evaluate_stratum_after_aggregates(
    Diagnostics, _, _, _, _, _, _, _, _, _, _, _, [], Diagnostics).

rule_at_level(Strata, Level, rule(call(Relation, _), _)) :-
    memberchk(stratum(Relation, Level), Strata).

%% demand_cone_rules(+Strata, +Level, +Rules, +Dependencies,
%%                   +CurrentRules, -PlainRules) is det.
%
% Install every current plain rule and the transitive definitions reached by
% its plain positive goals. Aggregate and negative goals read the completed
% lower-row snapshot, so their definitions do not enter this demand cone.
demand_cone_rules(
    Strata, Level, Rules, Dependencies, CurrentRules, PlainRules) :-
    exclude(aggregate_rule, CurrentRules, CurrentPlainRules),
    sort(CurrentPlainRules, Roots),
    demand_cone_dependency_index(Dependencies, DependencyIndex),
    demand_cone_rule_index(Strata, Level, Rules, RuleIndex),
    demand_cone_root_relations(Roots, RootRelations),
    demand_cone_relation_set(RootRelations, SeenRelations),
    list_to_assoc([], IncludedRelations),
    demand_cone_worklist(
        RootRelations, DependencyIndex, RuleIndex, SeenRelations,
        IncludedRelations, Roots, Selected),
    sort(Selected, PlainRules).

%% demand_cone_dependency_index(+Dependencies, -DependencyIndex) is det.
%
% The old fixpoint repeatedly rediscovered the same positive zero-gap body
% relations from every selected rule. Store that relation union once, keyed by
% its exact head relation. The final sort keeps the previous relation ordering.
demand_cone_dependency_index(Dependencies, DependencyIndex) :-
    findall(HeadRelation-BodyRelation,
            member(dependency(HeadRelation, BodyRelation,
                              positive, 0, positive), Dependencies),
            Pairs0),
    keysort(Pairs0, Pairs),
    group_pairs_by_key(Pairs, Groups),
    maplist(sort_relation_group, Groups, SortedGroups),
    list_to_assoc(SortedGroups, DependencyIndex).

sort_relation_group(Relation-Bodies0, Relation-Bodies) :-
    sort(Bodies0, Bodies).

%% demand_cone_rule_index(+Strata, +Level, +Rules, -RuleIndex) is det.
%
% Index every nonaggregate definition that the old include/3 scan could select
% for a newly discovered body relation. Rule list order is immaterial before
% the existing final sort/2, while retaining all same-head definitions keeps
% their dependency edges available to the worklist.
demand_cone_rule_index(Strata, Level, Rules, RuleIndex) :-
    findall(Relation-Rule,
            ( member(Rule, Rules),
              demand_cone_eligible_rule(Strata, Level, Rule, Relation)
            ),
            Pairs0),
    keysort(Pairs0, Pairs),
    group_pairs_by_key(Pairs, RuleGroups),
    list_to_assoc(RuleGroups, RuleIndex).

demand_cone_eligible_rule(Strata, Level, Rule, Relation) :-
    Rule = rule(call(Relation, _), _),
    memberchk(stratum(Relation, RuleLevel), Strata),
    RuleLevel =< Level,
    \+ aggregate_rule(Rule).

demand_cone_root_relations(Roots, RootRelations) :-
    findall(Relation,
            member(rule(call(Relation, _), _), Roots),
            RootRelations0),
    sort(RootRelations0, RootRelations).

demand_cone_relation_set(Relations, RelationSet) :-
    findall(Relation-true,
            member(Relation, Relations),
            Pairs),
    list_to_assoc(Pairs, RelationSet).

%% demand_cone_worklist(+Queue, +DependencyIndex, +RuleIndex,
%%                      +SeenRelations, +IncludedRelations, +Selected0,
%%                      -Selected) is det.
%
% Each relation head enters the queue once. A relation can be a current root
% before it later appears as a body relation, so SeenRelations and
% IncludedRelations are separate: the latter records when its eligible rule
% definitions have been added to Selected.
demand_cone_worklist([], _, _, _, _, Selected, Selected).
demand_cone_worklist(
    [Relation | Queue0], DependencyIndex, RuleIndex, Seen0, Included0,
    Selected0, Selected) :-
    (   get_assoc(Relation, DependencyIndex, BodyRelations)
    ->  true
    ;   BodyRelations = []
    ),
    demand_cone_discover(
        BodyRelations, RuleIndex, Seen0, Seen1, Included0, Included1,
        NewRelations, NewRules),
    append(Queue0, NewRelations, Queue),
    append(NewRules, Selected0, Selected1),
    demand_cone_worklist(
        Queue, DependencyIndex, RuleIndex, Seen1, Included1,
        Selected1, Selected).

demand_cone_discover([], _, Seen, Seen, Included, Included, [], []).
demand_cone_discover(
    [Relation | Relations], RuleIndex, Seen0, Seen, Included0, Included,
    NewRelations, NewRules) :-
    (   get_assoc(Relation, Seen0, _)
    ->  Seen1 = Seen0,
        NewRelations = NewRelations0
    ;   put_assoc(Relation, Seen0, true, Seen1),
        NewRelations = [Relation | NewRelations0]
    ),
    (   get_assoc(Relation, Included0, _)
    ->  Included1 = Included0,
        NewRules = NewRules0
    ;   put_assoc(Relation, Included0, true, Included1),
        (   get_assoc(Relation, RuleIndex, RelationRules)
        ->  append(RelationRules, NewRules0, NewRules)
        ;   NewRules = NewRules0
        )
    ),
    demand_cone_discover(
        Relations, RuleIndex, Seen1, Seen, Included1, Included,
        NewRelations0, NewRules0).

seed_at_level(Strata, Level, call(Relation, _)) :-
    relation_level(Strata, Relation, Level).

relation_level(Strata, Relation, Level) :-
    (   memberchk(stratum(Relation, DerivedLevel), Strata)
    ->  Level = DerivedLevel
    ;   Level = 0
    ).

%% current_result_relations(+Rules, +Seeds, -Relations) is det.
%%
% A stratum can add rows only through its own rule heads or seeds. Lower-level
% definitions remain available as body inputs through LowerRows and are unioned
% into the completed snapshot after the current roots are evaluated. The nil
% kernel relation is a permanent evaluator row, including empty strata.
current_result_relations(Rules, Seeds, Relations) :-
    findall(Relation,
            ( member(rule(call(Relation, _), _), Rules)
            ; member(call(Relation, _), Seeds)
            ),
            Relations0),
    sort([ref(kernel(nil)) | Relations0], Relations).

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
completed_body_holds(
    [checked_goal(positive, Call) | Goals], Rows) :-
    integer_comparison_arguments(Call, PositiveOperator, _, Left, Right),
    !,
    call(PositiveOperator, Left, Right),
    completed_body_holds(Goals, Rows).
completed_body_holds([checked_goal(positive, Call) | Goals], Rows) :-
    member(Call, Rows),
    completed_body_holds(Goals, Rows).
completed_body_holds(
    [checked_goal(negative, Call) | Goals], Rows) :-
    ground(Call),
    integer_comparison_arguments(Call, _, NegativeOperator, Left, Right),
    !,
    call(NegativeOperator, Left, Right),
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
    run_compile_step(
        evaluator, validate_functional_rows,
        validate_functional_rows_body(Relations, Rows, Diagnostics),
        validate_functional_rows_metrics(Relations, Rows, Diagnostics)).

validate_functional_rows_body(Relations, Rows, Diagnostics) :-
    sort(Rows, SortedRows),
    relation_key_diagnostics(Relations, SortedRows, Diagnostics0),
    sort(Diagnostics0, Diagnostics).

validate_functional_rows_metrics(
    Relations, Rows, Diagnostics,
    [ metric(validated_relations, RelationCount),
      metric(validated_rows, RowCount),
      metric(validated_diagnostics, DiagnosticCount)
    ]) :-
    length(Relations, RelationCount),
    length(Rows, RowCount),
    length(Diagnostics, DiagnosticCount).

relation_key_diagnostics(Relations, Rows, Diagnostics) :-
    rows_by_relation(Rows, RelationIndex),
    relation_key_diagnostics_indexed(Relations, RelationIndex, Diagnostics).

relation_key_diagnostics_indexed([], _, []).
relation_key_diagnostics_indexed(
    [relation(Relation, _, KeySets) | Relations],
    RelationIndex, Diagnostics) :-
    relation_index_rows(Relation, RelationIndex, RelationRows),
    key_sets_diagnostics(KeySets, Relation, RelationRows, OwnDiagnostics),
    relation_key_diagnostics_indexed(Relations, RelationIndex,
                                     RestDiagnostics),
    append(OwnDiagnostics, RestDiagnostics, Diagnostics).

%% rows_by_relation(+Rows, -RelationIndex) is det.
%
% Group one completed row set by relation name once, so each relation's key
% validation reads only its own rows instead of rescanning the whole closure.
rows_by_relation(Rows, RelationIndex) :-
    findall(Relation-Row,
            ( member(Row, Rows), Row = call(Relation, _) ),
            Pairs),
    keysort(Pairs, SortedPairs),
    group_pairs_by_key(SortedPairs, Groups),
    list_to_assoc(Groups, RelationIndex).

relation_index_rows(Relation, RelationIndex, RelationRows) :-
    (   get_assoc(Relation, RelationIndex, RelationRows)
    ->  true
    ;   RelationRows = []
    ).

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
    key_groups(Rows, Positions, Groups),
    member(Values-Group, Groups),
    ordered_row_pair(Group, Left, Right).

%% key_groups(+Rows, +Positions, -Groups) is det.
%
% Key each row by its functional key once, then stable-sort and group by that
% key. Only rows inside one equal-key group can conflict, so pairwise
% comparison stays within a group while row order keeps Left/Right orientation.
key_groups(Rows, Positions, Groups) :-
    findall(Key-Row,
            ( member(Row, Rows), key_values(Row, Positions, Key) ),
            Pairs),
    keysort(Pairs, SortedPairs),
    group_pairs_by_key(SortedPairs, Groups).

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
    stratify_rules_with_dependencies(
        Rules, Dependencies, DerivedStrata, Diagnostics).

stratify_rules_with_dependencies(
    Rules, Dependencies, DerivedStrata, Diagnostics) :-
    profile_occurrence(stratification_input, Rules),
    (   in_compile_scope
    ->  memoized_stratification(
            Rules, Dependencies, DerivedStrata, Diagnostics)
    ;   calc_stratification(
            Rules, Dependencies, DerivedStrata, Diagnostics)
    ).

%% memoized_stratification(+Rules, +Dependencies, -Strata, -Diagnostics) is det.
%
% Memoize the pure stratification result on the exact checked Rules term within
% the current compile scope. The term hash only selects a bucket; the stored
% rules term is compared exactly before a hit, so a hash collision misses.
memoized_stratification(Rules, Dependencies, DerivedStrata, Diagnostics) :-
    Key = stratification(Rules),
    term_hash(Key, Hash),
    (   compile_scope_memo_lookup(Hash, Key, Value)
    ->  Value = stratification_result(DerivedStrata, Diagnostics)
    ;   calc_stratification(Rules, Dependencies, DerivedStrata, Diagnostics),
        Value = stratification_result(DerivedStrata, Diagnostics),
        compile_scope_memo_store(Hash, Key, Value)
    ).

%% calc_stratification(+Rules, +Dependencies, -DerivedStrata, -Diagnostics)
%% is det.
%
% Debug instrumentation wraps the pure derivation: stratification input and
% result events are emitted only while debug tracing is active, and the
% worklist counters are kept outside the derived result.
calc_stratification(Rules, Dependencies, DerivedStrata, Diagnostics) :-
    debug_stratification_input(Rules, Dependencies),
    debug_reset_worklist_counters,
    calc_stratification_body(Rules, Dependencies, DerivedStrata, Diagnostics),
    debug_worklist_counters(Visits, Changes),
    debug_stratification_result(
        DerivedStrata, Diagnostics, Visits, Changes).

calc_stratification_body(
    Rules, Dependencies, DerivedStrata, Diagnostics) :-
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

debug_stratification_input(Rules, Dependencies) :-
    (   debug_trace_on
    ->  length(Rules, RuleCount),
        length(Dependencies, DependencyCount),
        rule_relations(Rules, Relations),
        length(Relations, RelationCount),
        dependency_polarity_counts(
            Dependencies, Positive, Negative, GapZero, GapOne),
        debug_event(stratification_input,
                    [phase=stratify, rules=RuleCount,
                     dependencies=DependencyCount, relations=RelationCount,
                     positive=Positive, negative=Negative,
                     gap_zero=GapZero, gap_one=GapOne])
    ;   true
    ).

dependency_polarity_counts(Dependencies, Positive, Negative,
                           GapZero, GapOne) :-
    aggregate_all(count,
                  member(dependency(_, _, positive, _, _), Dependencies),
                  Positive),
    aggregate_all(count,
                  member(dependency(_, _, negative, _, _), Dependencies),
                  Negative),
    aggregate_all(count,
                  member(dependency(_, _, _, 0, _), Dependencies),
                  GapZero),
    aggregate_all(count,
                  member(dependency(_, _, _, 1, _), Dependencies),
                  GapOne).

debug_stratification_result(DerivedStrata, Diagnostics, Visits, Changes) :-
    (   debug_trace_on
    ->  length(DerivedStrata, StrataCount),
        length(Diagnostics, DiagnosticCount),
        strict_cycle_count(Diagnostics, StrictCycleCount),
        strata_level_histogram(DerivedStrata, LevelHistogram),
        debug_histogram_fields(strata_levels, LevelHistogram, HistFields),
        append([phase=stratify, strata=StrataCount,
                diagnostics=DiagnosticCount,
                strict_cycles=StrictCycleCount,
                worklist_visits=Visits,
                level_changes=Changes], HistFields, Fields),
        debug_event(stratification_result, Fields)
    ;   true
    ).

strict_cycle_count(Diagnostics, Count) :-
    aggregate_all(count,
                  ( member(diagnostic(_, _, Reason), Diagnostics),
                    strict_cycle_reason(Reason)
                  ),
                  Count).

strict_cycle_reason(strict_dependency_cycle(_)).
strict_cycle_reason(aggregate_dependency_cycle(_)).

strata_level_histogram(Strata, Histogram) :-
    findall(Level, member(stratum(_, Level), Strata), Levels0),
    msort(Levels0, Levels),
    sort(Levels, DistinctLevels),
    level_counts(DistinctLevels, Levels, Histogram).

level_counts([], _, []).
level_counts([Level | Levels], All, [Level-Count | Rest]) :-
    aggregate_all(count, member(Level, All), Count),
    level_counts(Levels, All, Rest).

debug_reset_worklist_counters :-
    (   debug_trace_on
    ->  retractall(debug_worklist_visits(_)),
        retractall(debug_worklist_changes(_))
    ;   true
    ).

debug_worklist_counters(Visits, Changes) :-
    (   debug_trace_on
    ->  (   retract(debug_worklist_visits(Visits0))
        ->  Visits = Visits0
        ;   Visits = 0
        ),
        (   retract(debug_worklist_changes(Changes0))
        ->  Changes = Changes0
        ;   Changes = 0
        )
    ;   Visits = 0,
        Changes = 0
    ).

debug_bump_visit(true) :-
    (   retract(debug_worklist_visits(Count0))
    ->  Count is Count0 + 1
    ;   Count = 1
    ),
    assertz(debug_worklist_visits(Count)).
debug_bump_visit(false).

debug_bump_change(true) :-
    (   retract(debug_worklist_changes(Count0))
    ->  Count is Count0 + 1
    ;   Count = 1
    ),
    assertz(debug_worklist_changes(Count)).
debug_bump_change(false).

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

%% relax_to_fixpoint(+Dependencies, +Levels0, -Levels) is det.
%
% Solve level(Head) >= level(Body) + Gap for every dependency with the least
% fixpoint above the all-zero assignment. The dependency list is indexed by
% body relation once, and only readers of a relation whose level increased are
% relaxed, so each constraint is enforced on change instead of on every pass.
relax_to_fixpoint(Dependencies, Levels0, Levels) :-
    dependency_index(Dependencies, ByBody),
    dependency_bodies(Dependencies, Queue),
    relax_worklist(Queue, ByBody, Levels0, Levels).

%% dependency_index(+Dependencies, -ByBody) is det.
%
% ByBody groups (Head, Gap) under each BodyRelation that Head reads. A level
% increase of one body relation can only change the requirement of a relation
% that reads it, so the index bounds each relaxation step to those readers
% instead of rescanning all dependencies. Keys and values keep dependency
% order, so the fixpoint does not depend on traversal order.
dependency_index(Dependencies, ByBody) :-
    findall(BodyRelation-(HeadRelation-Gap),
            member(dependency(HeadRelation, BodyRelation, _, Gap, _),
                   Dependencies),
            Pairs0),
    keysort(Pairs0, Pairs),
    group_pairs_by_key(Pairs, Groups),
    list_to_assoc(Groups, ByBody).

%% dependency_bodies(+Dependencies, -Queue) is det.
%
% Seed the worklist with every body relation. Processing a body once enforces
% the initial gap on its readers even when its level never increases; later
% dequeues are driven only by level growth.
dependency_bodies(Dependencies, Queue) :-
    findall(BodyRelation,
            member(dependency(_, BodyRelation, _, _, _), Dependencies),
            Bodies0),
    sort(Bodies0, Queue).

%% relax_worklist(+Queue, +DependencyIndex, +Levels0, -Levels) is det.
%
% Changed-relation worklist. Queue holds relations whose level recently
% increased. Dequeuing a core relation relaxes only that relation's readers,
% and a reader is enqueued exactly when its level grows, so the queue drains
% once no requirement can change. Levels0 fixes the deterministic output order;
% the assoc carries relation lookup while relaxing.
relax_worklist(Queue, DependencyIndex, Levels0, Levels) :-
    level_index(Levels0, LevelByRelation0),
    (   debug_trace_on
    ->  Debug = true
    ;   Debug = false
    ),
    worklist_loop(Queue, DependencyIndex, LevelByRelation0, LevelByRelation,
                  Debug),
    levels_from_index(Levels0, LevelByRelation, Levels).

%% level_index(+Levels, -LevelByRelation) is det.
level_index(Levels, LevelByRelation) :-
    findall(Relation-Level,
            member(level(Relation, Level), Levels),
            Pairs),
    list_to_assoc(Pairs, LevelByRelation).

%% levels_from_index(+Levels0, +LevelByRelation, -Levels) is det.
%
% Rebuild the level list in the input relation order so the public strata
% output stays deterministic and independent of worklist scheduling.
levels_from_index([], _, []).
levels_from_index([level(Relation, _) | Levels0], LevelByRelation,
                  [level(Relation, Level) | Levels]) :-
    get_assoc(Relation, LevelByRelation, Level),
    levels_from_index(Levels0, LevelByRelation, Levels).

worklist_loop([], _, LevelByRelation, LevelByRelation, _).
worklist_loop([Relation | Queue0], DependencyIndex, LevelByRelation0,
              LevelByRelation, Debug) :-
    debug_bump_visit(Debug),
    get_assoc(Relation, LevelByRelation0, BodyLevel),
    (   get_assoc(Relation, DependencyIndex, Readers)
    ->  reader_levels(Readers, BodyLevel, LevelByRelation0, LevelByRelation1,
                      Enqueued, Debug),
        (   Enqueued == []
        ->  Queue = Queue0
        ;   append(Queue0, Enqueued, Queue)
        )
    ;   LevelByRelation1 = LevelByRelation0,
        Queue = Queue0
    ),
    worklist_loop(Queue, DependencyIndex, LevelByRelation1, LevelByRelation,
                  Debug).

reader_levels([], _, LevelByRelation, LevelByRelation, [], _).
reader_levels([HeadRelation-Gap | Readers], BodyLevel, LevelByRelation0,
              LevelByRelation, Enqueued, Debug) :-
    get_assoc(HeadRelation, LevelByRelation0, Current),
    Required is BodyLevel + Gap,
    (   Required > Current
    ->  debug_bump_change(Debug),
        put_assoc(HeadRelation, LevelByRelation0, Required, LevelByRelation1),
        reader_levels(Readers, BodyLevel, LevelByRelation1, LevelByRelation,
                      Rest, Debug),
        Enqueued = [HeadRelation | Rest]
    ;   reader_levels(Readers, BodyLevel, LevelByRelation0, LevelByRelation,
                      Enqueued, Debug)
    ).

strata_for_relations([], _, []).
strata_for_relations([Relation | Relations], Levels,
                     [stratum(Relation, Level) | Strata]) :-
    memberchk(level(Relation, Level), Levels),
    strata_for_relations(Relations, Levels, Strata).

install_evaluation(EvaluationId, Rules, Seeds, LowerRows, ClauseReferences) :-
    install_rules(Rules, EvaluationId, RuleReferences),
    install_seeds(Seeds, EvaluationId, SeedReferences),
    install_lower_rows(LowerRows, EvaluationId, LowerReferences),
    append([RuleReferences, SeedReferences, LowerReferences], ClauseReferences).

install_rules(Rules, EvaluationId, References) :-
    (   profile_scope_on
    ->  install_rules_profiled(Rules, EvaluationId, References)
    ;   install_rules_plain(Rules, EvaluationId, References)
    ).

install_rules_plain([], _, []).
install_rules_plain([Rule | Rules], EvaluationId, [Reference | References]) :-
    Rule = rule(call(Relation, _), _),
    assertz(evaluation_rule(EvaluationId, Relation, Rule), Reference),
    install_rules_plain(Rules, EvaluationId, References).

install_rules_profiled([], _, []).
install_rules_profiled([Rule | Rules], EvaluationId, [Reference | References]) :-
    Rule = rule(call(Relation, _), _),
    assertz(evaluation_rule(EvaluationId, Relation, Rule), Reference),
    profile_occurrence(evaluator_installed_rules, Rule),
    install_rules_profiled(Rules, EvaluationId, References).

install_seeds([], _, []).
install_seeds([Seed | Seeds], EvaluationId, [Reference | References]) :-
    (   profile_scope_on
    ->  install_seeds_profiled([Seed | Seeds], EvaluationId,
                               [Reference | References])
    ;   install_seeds_plain([Seed | Seeds], EvaluationId,
                           [Reference | References])
    ).

install_seeds_plain([], _, []).
install_seeds_plain([Seed | Seeds], EvaluationId, [Reference | References]) :-
    Seed = call(Relation, _),
    assertz(evaluation_seed(EvaluationId, Relation, Seed), Reference),
    install_seeds_plain(Seeds, EvaluationId, References).

install_seeds_profiled([], _, []).
install_seeds_profiled([Seed | Seeds], EvaluationId, [Reference | References]) :-
    Seed = call(Relation, _),
    assertz(evaluation_seed(EvaluationId, Relation, Seed), Reference),
    profile_occurrence(evaluator_installed_seeds, Seed),
    install_seeds_profiled(Seeds, EvaluationId, References).

install_lower_rows(Rows, EvaluationId, References) :-
    (   lower_store_scope(StoreId)
    ->  install_shared_lower_rows(Rows, StoreId, References)
    ;   install_private_lower_rows(Rows, EvaluationId, References)
    ).

install_private_lower_rows(Rows, EvaluationId, References) :-
    (   profile_scope_on
    ->  install_lower_rows_profiled(Rows, EvaluationId, References)
    ;   install_lower_rows_plain(Rows, EvaluationId, References)
    ).

%% install_shared_lower_rows(+Rows, +StoreId, -[]) is det.
%
% A stratum's completed lower snapshot is the union of every earlier snapshot.
% ord_subtract/3 keeps only the rows not yet stored for this evaluate call, so
% each distinct row is asserted once. The returned reference list is empty by
% design: the shared store is owned by the evaluate/4 scope, not by the
% stratum, so clear_evaluation/2 must not erase it.
install_shared_lower_rows(Rows, StoreId, []) :-
    (   retract(lower_store_installed(StoreId, Installed))
    ->  true
    ;   Installed = []
    ),
    ord_subtract(Rows, Installed, NewRows),
    assertz(lower_store_installed(StoreId, Rows)),
    (   profile_scope_on
    ->  install_lower_rows_profiled(NewRows, StoreId, _)
    ;   install_lower_rows_plain(NewRows, StoreId, _)
    ).

install_lower_rows_plain([], _, []).
install_lower_rows_plain([Row | Rows], EvaluationId, [Reference | References]) :-
    Row = call(Relation, Arguments),
    index_argument_hashes(Arguments, Hash1, Hash2, Hash3, Hash4),
    assertz(evaluation_lower_index(
                EvaluationId, Relation, Hash1, Hash2, Hash3, Hash4, Row),
            Reference),
    install_lower_rows_plain(Rows, EvaluationId, References).

install_lower_rows_profiled([], _, []).
install_lower_rows_profiled([Row | Rows], EvaluationId, [Reference | References]) :-
    Row = call(Relation, Arguments),
    index_argument_hashes(Arguments, Hash1, Hash2, Hash3, Hash4),
    assertz(evaluation_lower_index(
                EvaluationId, Relation, Hash1, Hash2, Hash3, Hash4, Row),
            Reference),
    profile_occurrence(evaluator_installed_lower_rows, Row),
    install_lower_rows_profiled(Rows, EvaluationId, References).

%% evaluation_lower(+EvaluationId, +Relation, ?Row) is nondet.
%
% Normal lookup wrapper over the one evaluation_lower_index/7 store. A ground
% argument contributes its term hash to the lookup; an unknown or absent
% argument leaves that hash position unbound. The hashes only narrow the
% candidates: the stored full row is unified exactly afterwards, so a hash
% collision cannot change an answer. With Row or Relation unbound the index
% enumerates every stored row for that relation, so the wrapper stays
% nondeterministic in the modes existing callers use.
evaluation_lower(EvaluationId, Relation, Row) :-
    Row = call(_, Arguments),
    lower_store_id(EvaluationId, StoreId),
    index_argument_hashes(Arguments, Hash1, Hash2, Hash3, Hash4),
    evaluation_lower_index(StoreId, Relation,
                           Hash1, Hash2, Hash3, Hash4, Stored),
    Stored = Row.

index_argument_hashes(Arguments, Hash1, Hash2, Hash3, Hash4) :-
    index_argument_hash(0, Arguments, Hash1),
    index_argument_hash(1, Arguments, Hash2),
    index_argument_hash(2, Arguments, Hash3),
    index_argument_hash(3, Arguments, Hash4).

index_argument_hash(Position, Arguments, Hash) :-
    (   nonvar(Arguments),
        nth0(Position, Arguments, Argument),
        ground(Argument)
    ->  term_hash(Argument, Hash)
    ;   true
    ).

%% collect_closure(+EvaluationId, +ResultRelations, +LowerRows, -Closure)
%% is det.
%%
% Bound relation roots enumerate only rows that can be produced in this
% stratum. Completed lower rows are already immutable and are merged afterward,
% avoiding a fresh unbound proves/2 traversal over every lower relation.
collect_closure(EvaluationId, ResultRelations, LowerRows, Closure) :-
    findall(Call,
            ( member(Relation, ResultRelations),
              Call = call(Relation, _),
              proves(EvaluationId, Call)
            ),
            Calls),
    sort(Calls, CurrentRows),
    findall(Request, evaluation_request(EvaluationId, Request), Requests),
    sort(Requests, RequestRows),
    ord_union(CurrentRows, RequestRows, NewRows),
    ord_union(LowerRows, NewRows, Closure),
    profile_closure_rows(NewRows).

% The profile category records the rows collected for this stratum before the
% completed lower snapshot is merged, so repeated lower-row enumeration is
% visible as removed work rather than as repeated output accounting.
profile_closure_rows(NewRows) :-
    (   profile_scope_on
    ->  profile_closure_rows_(NewRows)
    ;   true
    ).

profile_closure_rows_([]).
profile_closure_rows_([Row | Rows]) :-
    profile_occurrence(evaluator_collected_closure_rows, Row),
    profile_closure_rows_(Rows).

clear_evaluation(EvaluationId, ClauseReferences) :-
    abolish_table_subgoals(dl7_evaluator:proves(EvaluationId, _)),
    retractall(evaluation_request(EvaluationId, _)),
    maplist(erase, ClauseReferences).

%% Evaluate-step metrics, called by run_compile_step/4 outside the measured
%% interval and only when an active compile trace has DL7_TRACE step collection
%% enabled. Install counts come from the known input lists. Collect reads the
%% process-global SLG table counters (table_statistics/2 spans all tables)
%% while the tables are still live, before cleanup tears them down:
%%   answers              total answers across all answer tries
%%   complete_call        times answers were generated from a completed table,
%%                        i.e. answer reuse (not a per-stratum cache-hit count)
%%   space                summed answer-trie memory in bytes
%% Cleanup reports the known clause-reference count and a per-EvaluationId leak
%% check.
evaluate_install_metrics(Level, Rules, Seeds, LowerRows,
                         [ metric(stratum_rules, RuleCount),
                           metric(stratum_seeds, SeedCount),
                           metric(stratum_lower_rows, LowerRowCount)
                         ]) :-
    length(Rules, RuleCount),
    length(Seeds, SeedCount),
    length(LowerRows, LowerRowCount),
    debug_evaluator_install(Level, Rules, Seeds, LowerRows).

evaluate_collect_metrics(Level, CompletedRows,
                         [ metric(stratum_closure_rows, ClosureCount),
                           metric(global_table_answers, TableAnswers),
                           metric(global_complete_calls, TableCompleteCalls),
                           metric(global_table_space_bytes, TableSpaceBytes)
                         ]) :-
    length(CompletedRows, ClosureCount),
    table_statistics(answers, TableAnswers),
    table_statistics(complete_call, TableCompleteCalls),
    table_statistics(space, TableSpaceBytes),
    debug_evaluator_collect(Level, CompletedRows).

evaluate_cleanup_metrics(Level, EvaluationId, ClauseReferences,
                         [ metric(erased_clauses, ErasedCount),
                           metric(leftover_lower_rows, LeftoverRowCount)
                         ]) :-
    length(ClauseReferences, ErasedCount),
    aggregate_all(count,
                  evaluation_lower_index(EvaluationId, _, _, _, _, _, _),
                  LeftoverRowCount),
    debug_evaluator_cleanup(Level, EvaluationId, ClauseReferences).

%% Evaluator debug events. Per-relation cardinalities and global table
%% statistics are gathered only while debug tracing is active, after the
%% measured install/collect/cleanup interval.

debug_evaluator_install(Level, Rules, Seeds, LowerRows) :-
    (   debug_trace_on
    ->  length(Rules, RuleCount),
        length(Seeds, SeedCount),
        length(LowerRows, LowerRowCount),
        relation_rule_histogram(Rules, RuleHistogram),
        relation_call_histogram(Seeds, SeedHistogram),
        relation_call_histogram(LowerRows, LowerHistogram),
        debug_histogram_fields(rule_relations, RuleHistogram, RuleFields),
        debug_histogram_fields(seed_relations, SeedHistogram, SeedFields),
        debug_histogram_fields(lower_relations, LowerHistogram, LowerFields),
        append([phase=evaluator, stratum=Level, outcome=installed,
                rules=RuleCount, seeds=SeedCount,
                lower_rows=LowerRowCount | RuleFields],
               SeedFields, Fields0),
        append(Fields0, LowerFields, Fields),
        debug_event(evaluator_install, Fields)
    ;   true
    ).

debug_evaluator_collect(Level, CompletedRows) :-
    (   debug_trace_on
    ->  length(CompletedRows, ClosureCount),
        relation_call_histogram(CompletedRows, ClosureHistogram),
        debug_histogram_fields(
            closure_relations, ClosureHistogram, ClosureFields),
        table_statistics(answers, TableAnswers),
        table_statistics(complete_call, TableCompleteCalls),
        table_statistics(space, TableSpaceBytes),
        append([phase=evaluator, stratum=Level, outcome=collected,
                closure_rows=ClosureCount,
                global_table_answers=TableAnswers,
                global_complete_calls=TableCompleteCalls,
                global_table_space_bytes=TableSpaceBytes | ClosureFields],
               [], Fields),
        debug_event(evaluator_collect, Fields)
    ;   true
    ).

debug_evaluator_cleanup(Level, EvaluationId, ClauseReferences) :-
    (   debug_trace_on
    ->  length(ClauseReferences, ErasedCount),
        aggregate_all(count,
                      evaluation_lower_index(EvaluationId, _, _, _, _, _, _),
                      LeftoverRowCount),
        debug_event(evaluator_cleanup,
                    [phase=evaluator, stratum=Level, outcome=cleared,
                     erased_clauses=ErasedCount,
                     leftover_lower_rows=LeftoverRowCount])
    ;   true
    ).

relation_rule_histogram(Rules, Histogram) :-
    findall(Relation,
            ( member(rule(call(Relation, _), _), Rules) ),
            Relations0),
    sort(Relations0, Relations),
    relation_rule_counts(Relations, Rules, Histogram).

relation_rule_counts([], _, []).
relation_rule_counts([Relation | Relations], Rules, [Relation-Count | Rest]) :-
    aggregate_all(count,
                  member(rule(call(Relation, _), _), Rules),
                  Count),
    relation_rule_counts(Relations, Rules, Rest).

relation_call_histogram(Rows, Histogram) :-
    findall(Relation, member(call(Relation, _), Rows), Relations0),
    sort(Relations0, Relations),
    relation_call_counts(Relations, Rows, Histogram).

relation_call_counts([], _, []).
relation_call_counts([Relation | Relations], Rows, [Relation-Count | Rest]) :-
    aggregate_all(count, member(call(Relation, _), Rows), Count),
    relation_call_counts(Relations, Rows, Rest).

proves(_, Call) :-
    integer_comparison_arguments(Call, PositiveOperator, _, Left, Right),
    !,
    call(PositiveOperator, Left, Right).
proves(EvaluationId, Call) :-
    Call = call(Relation, _),
    evaluation_seed(EvaluationId, Relation, Call).
proves(EvaluationId, Call) :-
    Call = call(Relation, _),
    evaluation_lower(EvaluationId, Relation, Call).
proves(_, call(ref(kernel(nil)), [const([])])).
proves(EvaluationId, Head) :-
    Head = call(Relation, _),
    evaluation_rule(EvaluationId, Relation, Rule),
    instantiate_rule(Rule, Head, Body),
    proves_body(Body, EvaluationId).
proves(_, call(ref(kernel(cons)), [Head, Tail, List])) :-
    cons_relation(Head, Tail, List).
proves(_, call(ref(kernel(edge_ref)),
               [ref(Owner), Label, ref(edge(Owner, SemanticLabel))])) :-
    ground(Owner),
    ground(Label),
    semantic_argument(Label, SemanticLabel).
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
satisfy_goal(_, Goal) :-
    goal_call(Goal, negative, Call),
    ground(Call),
    integer_comparison_arguments(Call, _, NegativeOperator, Left, Right),
    !,
    call(NegativeOperator, Left, Right).
satisfy_goal(EvaluationId, Goal) :-
    goal_call(Goal, negative, Call),
    ground(Call),
    Call = call(Relation, _),
    \+ evaluation_lower(EvaluationId, Relation, Call).

%% kernel_int_lt_call(+Call) is semidet.
kernel_int_lt_call(Call) :-
    Call = call(ref(kernel(int_lt)), _),
    integer_comparison_call(Call).

%% integer_comparison_call(+Call) is semidet.
integer_comparison_call(Call) :-
    integer_comparison_arguments(Call, PositiveOperator, _, Left, Right),
    call(PositiveOperator, Left, Right).

integer_comparison_arguments(
    call(ref(kernel(Name)), [const(Left), const(Right)]),
    PositiveOperator, NegativeOperator, Left, Right) :-
    integer_comparison(Name, PositiveOperator, NegativeOperator),
    integer(Left),
    integer(Right).

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
