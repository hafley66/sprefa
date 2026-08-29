:- module(dl7_evaluator,
          [ evaluate/4,
            stratify_rules/3,
            validate_functional_rows/3
          ]).

:- use_module(library(error), [must_be/2]).
:- use_module(library(gensym), [gensym/2]).
:- use_module(library(lists), [max_list/2]).
:- use_module(library(ugraphs),
              [ neighbors/3,
                transitive_closure/2,
                vertices_edges_to_ugraph/3
              ]).

:- dynamic evaluation_rule/2.
:- dynamic evaluation_seed/2.

:- table proves/2.

%% evaluate(+Rules, +Seeds, -Closure, -Diagnostics) is det.
%
% Close one ground positive Datalog program. Compiler and runtime callers use
% the same entry point and checked-goal representation. Mutable clauses and
% SLG tables carry a fresh evaluation identity and are removed on every exit.
evaluate(Rules, Seeds, Closure, Diagnostics) :-
    must_be(ground, Rules),
    must_be(ground, Seeds),
    gensym(dl7_evaluation_, EvaluationId),
    setup_call_cleanup(
        install_evaluation(EvaluationId, Rules, Seeds, ClauseReferences),
        collect_closure(EvaluationId, Closure),
        clear_evaluation(EvaluationId, ClauseReferences)),
    Diagnostics = [].

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
    ordered_row_pair(Rows, Left, Right),
    key_values(Left, Positions, Values),
    key_values(Right, Positions, Values).

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
rule_dependencies([rule(call(HeadRelation, _), Goals) | Rules], Dependencies) :-
    goal_dependencies(Goals, HeadRelation, OwnDependencies),
    rule_dependencies(Rules, RestDependencies),
    append(OwnDependencies, RestDependencies, Dependencies).

goal_dependencies([], _, []).
goal_dependencies(
    [checked_goal(Polarity, call(BodyRelation, _)) | Goals], HeadRelation,
    [dependency(HeadRelation, BodyRelation, Polarity, Gap) | Dependencies]) :-
    polarity_gap(Polarity, Gap),
    goal_dependencies(Goals, HeadRelation, Dependencies).

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
    findall(HeadRelation-BodyRelation,
            ( member(dependency(HeadRelation, BodyRelation, _, 1),
                     Dependencies),
              neighbors(BodyRelation, Closure, Reachable),
              memberchk(HeadRelation, Reachable)
            ),
            StrictEdges0),
    sort(StrictEdges0, StrictEdges),
    strict_edges_diagnostics(StrictEdges, Diagnostics).

strict_edges_diagnostics([], []) :- !.
strict_edges_diagnostics(StrictEdges,
                         [diagnostic(stratify, none,
                                     strict_dependency_cycle(StrictEdges))]).

dependency_edges([], []).
dependency_edges([dependency(HeadRelation, BodyRelation, _, _) | Dependencies],
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
            ( member(dependency(Relation, BodyRelation, _, Gap),
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

install_evaluation(EvaluationId, Rules, Seeds, ClauseReferences) :-
    install_rules(Rules, EvaluationId, RuleReferences),
    install_seeds(Seeds, EvaluationId, SeedReferences),
    append(RuleReferences, SeedReferences, ClauseReferences).

install_rules([], _, []).
install_rules([Rule | Rules], EvaluationId, [Reference | References]) :-
    assertz(evaluation_rule(EvaluationId, Rule), Reference),
    install_rules(Rules, EvaluationId, References).

install_seeds([], _, []).
install_seeds([Seed | Seeds], EvaluationId, [Reference | References]) :-
    assertz(evaluation_seed(EvaluationId, Seed), Reference),
    install_seeds(Seeds, EvaluationId, References).

collect_closure(EvaluationId, Closure) :-
    findall(Call, proves(EvaluationId, Call), Calls),
    sort(Calls, Closure).

clear_evaluation(EvaluationId, ClauseReferences) :-
    abolish_table_subgoals(dl7_evaluator:proves(EvaluationId, _)),
    maplist(erase, ClauseReferences).

proves(EvaluationId, Call) :-
    evaluation_seed(EvaluationId, Call).
proves(EvaluationId, Head) :-
    evaluation_rule(EvaluationId, Rule),
    instantiate_rule(Rule, Head, Body),
    proves_body(Body, EvaluationId).
proves(_, call(ref(kernel(cons)), [Head, Tail, List])) :-
    ground(Head),
    ground(Tail),
    cons_value(Head, Tail, List).
proves(_, call(ref(kernel(intern)), [Constructor, Arguments, Result])) :-
    ground(Constructor),
    ground(Arguments),
    intern_value(Constructor, Arguments, Result).

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

cons_value(Head, const(symbol(nil)), const([Head])).
cons_value(Head, const(Tail), const([Head | Tail])) :-
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
instantiate_argument(Argument, Variables, Variables, Argument).

variable_for_identity(Identity, Variables0, Variables, Variable) :-
    (   memberchk(Identity-Existing, Variables0)
    ->  Variable = Existing,
        Variables = Variables0
    ;   Variables = [Identity-Variable | Variables0]
    ).
