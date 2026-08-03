% 2_demand_cone.plt : receipts for demand_cone/4 (the lazy-compute seed).
%
% One predicate, compute only, wired to nothing yet. These pin the two
% branches of the spec: the null-query compat rule, and the demand-dependency
% closure (transitive, positive and negated reads, sampler wrappers included).

:- use_module(library(plunit)).
:- use_module(library(lists)).
:- use_module('../../2_demand_cone', [ demand_cone/4 ]).
:- use_module('../../compile', [ program_plan/2 ]).
:- use_module('../../lower', [ lower_program/2, boot_statements/5 ]).
:- use_module('../../emit_ts', [ emit_program/5 ]).
:- use_module('../parse_dl', [ parse_dl/4 ]).

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

% ── the wire: the cone reaches the emitted module ────────────────────────────

% The hand-computed cone of this program: the query seeds job/2, job's one rule
% body reads seed/2, and unread/1 is declared and reachable from nothing. The
% emitted constant is what a lazy consumer would read; nothing consumes it yet,
% so this line is the whole receipt that the compiler computed it at all.
test(emitted_module_carries_the_hand_computed_cone) :-
    string_codes(
      "rel seed(id: int, secs: int).\nrel job(id: int, secs: int).\nrel unread(id: int).\njob(Id, Secs) <- seed(Id, Secs).\n? job(7, secs).\n",
      Codes),
    parse_dl(Codes, Program, Bindings, []),
    emitted_text(emitted_module_carries_the_hand_computed_cone,
                 Program, Bindings, Text),
    once(sub_atom(Text, _, _, _,
                  'export const demandedRels: readonly string[] = ["job/2", "seed/2"];')),
    !.

% The compat rule at the emit seam: no query decl means no analysis entry
% point, so every rel the program declares or mentions is on the demand side.
test(zero_query_module_carries_every_rel) :-
    string_codes(
      "rel seed(id: int, secs: int).\nrel job(id: int, secs: int).\nrel unread(id: int).\njob(Id, Secs) <- seed(Id, Secs).\n",
      Codes),
    parse_dl(Codes, Program, Bindings, []),
    emitted_text(zero_query_module_carries_every_rel, Program, Bindings, Text),
    once(sub_atom(Text, _, _, _,
                  'export const demandedRels: readonly string[] = ["job/2", "seed/2", "unread/1"];')),
    !.

emitted_text(Name, Program, Bindings, Text) :-
    program_plan(fixture(Name, Program, [], [], [])-Bindings, Plan),
    lower_program(Plan, Lowered),
    Plan = plan(_, prog(Decls, _), RelPlans, _, _, _, _),
    Lowered = lowered(_, _, _, _, LevelStatements, _, _, _),
    boot_statements(Decls, RelPlans, [], LevelStatements, Boot),
    emit_program(Name, Plan, Lowered, Boot, Text).

:- end_tests(demand_cone).
