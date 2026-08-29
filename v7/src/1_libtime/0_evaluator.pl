:- module(dl7_evaluator, [evaluate/4]).

:- use_module(library(error), [must_be/2]).
:- use_module(library(gensym), [gensym/2]).

:- dynamic evaluation_rule/2.
:- dynamic evaluation_seed/2.

:- table proves/2.

%% evaluate(+Rules, +Seeds, -Closure, -Diagnostics) is det.
%
% Close one ground positive Datalog program. Compiler and runtime callers use
% the same entry point and checked call representation. Mutable clauses and
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
proves_body([Call | Calls], EvaluationId) :-
    proves(EvaluationId, Call),
    proves_body(Calls, EvaluationId).

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
    instantiate_calls(Body0, Variables0, _, Body).

instantiate_calls([], Variables, Variables, []).
instantiate_calls([Call0 | Calls0], Variables0, Variables,
                  [Call | Calls]) :-
    instantiate_call(Call0, Variables0, Variables1, Call),
    instantiate_calls(Calls0, Variables1, Variables, Calls).

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
