% 2_demand_cone.plt : receipts for demand_cone/4 (the lazy-compute seed).
%
% One predicate, compute only, wired to nothing yet. These pin the two
% branches of the spec: the null-query compat rule, and the demand-dependency
% closure (transitive, positive and negated reads, sampler wrappers included).

:- use_module(library(plunit)).
:- use_module(library(lists)).
:- use_module('../../2_demand_cone', [ demand_cone/4 ]).

:- begin_tests(demand_cone).

% An empty query list means no analysis entry point; the compat rule puts the
% whole program on the demand side, so every declared rel is in the cone.
test(zero_query_all_rels) :-
    Decls = [kind(root/1, log), keep(root/1, all),
             kind(mid/2, set),
             kind(top/1, set)],
    Rules = [(mid(Left, Right) <- root(Left), root(Right)),
             (top(Result) <- mid(_, Result))],
    demand_cone(Decls, Rules, [], Cone),
    Cone == [mid/2, root/1, top/1].

% The cone is only the query's demand chain; the rule nowhere reachable from
% the seed (e <- d) stays out even though d and e are declared.
test(hand_computed_cone) :-
    Decls = [kind(a/1, set), kind(b/1, set), kind(c/1, set),
             kind(d/1, set), kind(e/1, set)],
    Rules = [(b(Value) <- a(Value)),
             (c(Value) <- b(Value)),
             (e(Value) <- d(Value))],
    demand_cone(Decls, Rules, [c(Value)], Cone),
    Cone == [a/1, b/1, c/1].

% pre/1 still names its sampled rel, so the sampler reference joins the cone
% alongside the bare read that feeds it.
test(sampler_included) :-
    Decls = [kind(trigger/1, log), keep(trigger/1, all),
             kind(sample/1, set)],
    Rules = [(sample(Total) <- trigger(Item), pre(sample(Total)))],
    demand_cone(Decls, Rules, [sample(Item)], Cone),
    sort([sample/1, trigger/1], Expect),
    Cone == Expect.

% A negated body read is still a dependency: the negated rel joins the cone.
test(negation_included) :-
    Decls = [kind(ok/1, set), kind(blocked/1, set),
             kind(gate/1, set)],
    Rules = [(gate(Name) <- ok(Name), not(blocked(Name)))],
    demand_cone(Decls, Rules, [gate(Name)], Cone),
    sort([gate/1, ok/1, blocked/1], Expect),
    Cone == Expect.

:- end_tests(demand_cone).
