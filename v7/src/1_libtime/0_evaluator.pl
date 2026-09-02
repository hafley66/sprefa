:- module(dl7_evaluator,
          [ derive_aggregate_rows/4,
            evaluate/4,
            evaluate_stratified/5,
            stratify_rules/3,
            validate_functional_rows/3
          ]).

:- use_module(library(error), [must_be/2]).
:- use_module(library(gensym), [gensym/2]).
:- use_module(library(lists), [max_list/2]).
:- use_module(library(ordsets), [ord_subtract/3]).
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
    gensym(dl7_evaluation_, EvaluationId),
    setup_call_cleanup(
        true,
        evaluate_strata(0, MaxStratum, EvaluationId,
                        Strata, Rules, Seeds, [], [],
                        Closure, Diagnostics),
        clear_evaluation(EvaluationId)).
evaluate_after_stratify(Diagnostics, _, _, _, [], Diagnostics).

max_stratum([], 0).
max_stratum(Strata, MaxStratum) :-
    findall(Level, member(stratum(_, Level), Strata), Levels),
    max_list(Levels, MaxStratum).

evaluate_strata(Level, MaxStratum, _, _, _, _, Closure, _, Closure, []) :-
    Level > MaxStratum,
    !.
evaluate_strata(Level, MaxStratum, EvaluationId,
                Strata, Rules, Seeds, LowerRows, InstalledLowerRows,
                Closure, Diagnostics) :-
    include(rule_at_level(Strata, Level), Rules, CurrentRules),
    include(seed_at_level(Strata, Level), Seeds, CurrentSeeds),
    include(aggregate_rule, CurrentRules, AggregateRules),
    exclude(aggregate_rule, CurrentRules, PlainRules),
    derive_aggregate_rule_rows(LowerRows, AggregateRules,
                               AggregateSeeds, AggregateDiagnostics),
    evaluate_stratum_after_aggregates(
        AggregateDiagnostics, AggregateSeeds,
        Level, MaxStratum, EvaluationId,
        Strata, Rules, Seeds, LowerRows, InstalledLowerRows,
        PlainRules, CurrentSeeds, Closure, Diagnostics).

evaluate_stratum_after_aggregates(
    [], AggregateSeeds,
    Level, MaxStratum, EvaluationId,
    Strata, Rules, Seeds, LowerRows, InstalledLowerRows,
    PlainRules, CurrentSeeds, Closure, Diagnostics) :-
    !,
    append(CurrentSeeds, AggregateSeeds, Seeds0),
    sort(Seeds0, StratumSeeds),
    ord_subtract(LowerRows, InstalledLowerRows, NewLowerRows),
    install_rules(PlainRules, EvaluationId),
    install_seeds(StratumSeeds, EvaluationId),
    install_lower_rows(NewLowerRows, EvaluationId),
    abolish_table_subgoals(dl7_evaluator:proves(EvaluationId, _)),
    collect_closure(EvaluationId, CompletedRows),
    NextLevel is Level + 1,
    evaluate_strata(NextLevel, MaxStratum, EvaluationId,
                    Strata, Rules, Seeds, CompletedRows, LowerRows,
                    Closure, Diagnostics).
evaluate_stratum_after_aggregates(
    Diagnostics, _, _, _, _, _, _, _, _, _, _, _, _,
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
    relation_key_diagnostics(Relations, SortedRows, Diagnostics0),
    sort(Diagnostics0, Diagnostics).

relation_key_diagnostics([], _, []).
relation_key_diagnostics([relation(Relation, _, KeySets) | Relations], Rows,
                         Diagnostics) :-
    relation_rows(Relation, Rows, RelationRows),
    key_sets_diagnostics(KeySets, Relation, RelationRows, OwnDiagnostics),
    relation_key_diagnostics(Relations, Rows, RestDiagnostics),
    append(OwnDiagnostics, RestDiagnostics, Diagnostics).

relation_rows(_, [], []).
relation_rows(Relation, [call(Relation, Arguments) | Rows],
              [call(Relation, Arguments) | RelationRows]) :-
    !,
    relation_rows(Relation, Rows, RelationRows).
relation_rows(Relation, [_ | Rows], RelationRows) :-
    relation_rows(Relation, Rows, RelationRows).

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
    relax_levels(Levels0, Dependencies, Levels1),
    (   Levels1 == Levels0
    ->  Levels = Levels1
    ;   relax_to_fixpoint(Dependencies, Levels1, Levels)
    ).

relax_levels(Levels0, Dependencies, Levels) :-
    relax_levels(Levels0, Levels0, Dependencies, Levels).

relax_levels([], _, _, []).
relax_levels([level(Relation, Current) | Levels0], AllLevels, Dependencies,
             [level(Relation, Next) | Levels]) :-
    dependency_requirements(Relation, Dependencies,
                            AllLevels, Requirements0),
    Requirements = [Current | Requirements0],
    max_list(Requirements, Next),
    relax_levels(Levels0, AllLevels, Dependencies, Levels).

dependency_requirements(Relation, Dependencies, Levels, Requirements) :-
    findall(Required,
            ( member(dependency(Relation, BodyRelation, _, Gap, _),
                     Dependencies),
              memberchk(level(BodyRelation, BodyLevel), Levels),
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
    assertz(evaluation_rule(EvaluationId, Relation, Head, Body)),
    install_rules(Rules, EvaluationId).

install_seeds([], _).
install_seeds([Seed | Seeds], EvaluationId) :-
    Seed = call(Relation, _),
    assertz(evaluation_seed(EvaluationId, Relation, Seed)),
    install_seeds(Seeds, EvaluationId).

install_lower_rows([], _).
install_lower_rows([Row | Rows], EvaluationId) :-
    Row = call(Relation, _),
    assertz(evaluation_lower(EvaluationId, Relation, Row)),
    install_lower_rows(Rows, EvaluationId).

collect_closure(EvaluationId, Closure) :-
    findall(Call, proves(EvaluationId, Call), Calls),
    findall(Request, evaluation_request(EvaluationId, Request), Requests),
    append(Calls, Requests, Rows),
    sort(Rows, Closure).

clear_evaluation(EvaluationId) :-
    abolish_table_subgoals(dl7_evaluator:proves(EvaluationId, _)),
    retractall(evaluation_request(EvaluationId, _)),
    retractall(evaluation_rule(EvaluationId, _, _, _)),
    retractall(evaluation_seed(EvaluationId, _, _)),
    retractall(evaluation_lower(EvaluationId, _, _)).

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
