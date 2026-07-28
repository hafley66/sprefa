% redteam_kernel.pl : adversarial probes against the switch_flow HEADLINE,
% "the minimal stored kernel is ZERO engine rels and zero new tick phases".
%
% Run:  swipl -q -l v6/prolog/labs/redteam_kernel.pl -g go -g halt
%
% Claim under attack: plans/2026-07-27-switch-flow.md sections 7 to 9.
% Lab it came from: `git show ac2aafdc:v6/prolog/labs/switch_flow.pl` (89/89).
% Verdicts: plans/2026-07-27-redteam-minimal-kernel.md.
%
% SELF-CONTAINED ON PURPOSE. switch_flow.pl is deleted per the lab protocol, so
% this file carries its own minimal reference closure rather than depending on a
% file that does not exist on main. The closure here is the same shape as
% conformance/engine.pl level_step: naive iterate-to-fixpoint over ground rules,
% with a ROUND COUNTER added, which is the whole point of section A1.
%
% Every round count below is a PROXY for a lowering-tier statement count. It is
% named a proxy every time it is used. A Prolog interpreter cannot measure
% SQLite statements; it can measure how many times a delta must cross a rule
% boundary, and that number is what a semi-naive lowering turns into rounds.

:- use_module('../src/grader.pl').
:- use_module(library(lists)).
:- use_module(library(apply)).

:- op(1150, xfx, <-).
:- discontiguous check/2.

% ═══════════════════════════════════════════════════════════════════════════
% MINI ENGINE: ground level closure with a round counter
% ═══════════════════════════════════════════════════════════════════════════

solve(true, _) :- !.
solve((Left, Right), Visible) :- !, solve(Left, Visible), solve(Right, Visible).
solve(not(Goal), Visible) :- !, \+ solve(Goal, Visible).
solve(Goal, _) :- comparison_goal(Goal), !, ground(Goal), call(Goal).
solve(Atom, Visible) :- member(Atom, Visible).

comparison_goal(Goal) :-
    functor(Goal, Name, Arity),
    memberchk(Name/Arity, [ (<)/2, (>)/2, (=<)/2, (>=)/2, (==)/2, (\==)/2 ]).

body_atoms((Left, Right), Atoms) :- !,
    body_atoms(Left, LeftAtoms), body_atoms(Right, RightAtoms),
    append(LeftAtoms, RightAtoms, Atoms).
body_atoms(not(_), []) :- !.
body_atoms(true, []) :- !.
body_atoms(Goal, []) :- comparison_goal(Goal), !.
body_atoms(Atom, [Atom]).

% closure(+Rules, +Base, -Level, -Rounds). Rounds counts the iterations a
% semi-naive lowering would have to run, INCLUDING the final no-growth round.
closure(Rules, Base, Level, Rounds) :- closure_step(Rules, Base, [], 0, Level, Rounds).

closure_step(Rules, Base, Known, RoundsSoFar, Level, Rounds) :-
    append(Base, Known, Visible0), sort(Visible0, Visible),
    findall(Head,
            ( member((Head0 <- Body0), Rules),
              copy_term((Head0 <- Body0), (Head <- Body)),
              solve(Body, Visible) ),
            Heads),
    append(Known, Heads, Merged0), sort(Merged0, Merged),
    NextRounds is RoundsSoFar + 1,
    (   Merged == Known
    ->  Level = Known, Rounds = NextRounds
    ;   closure_step(Rules, Base, Merged, NextRounds, Level, Rounds) ).

% one grounded derivation of Head from Rules against a fully known model
derivation(Rules, Model, Head, BodyAtoms) :-
    member((Head0 <- Body0), Rules),
    copy_term((Head0 <- Body0), (Head <- Body)),
    solve(Body, Model),
    body_atoms(Body, BodyAtoms).

% ═══════════════════════════════════════════════════════════════════════════
% DRed / counting deletion, as a round-counting proxy
% ═══════════════════════════════════════════════════════════════════════════
% Over-deletion: a derived fact dies this round if ANY of its derivations used a
% fact that died in an earlier round. Rederivation: a dead fact comes back if it
% still has one derivation entirely inside the survivors.

overdelete(Rules, Model, Seed, Dead, Rounds) :-
    overdelete_step(Rules, Model, Seed, 0, Dead, Rounds).

overdelete_step(Rules, Model, Dead0, RoundsSoFar, Dead, Rounds) :-
    findall(Head,
            ( derivation(Rules, Model, Head, BodyAtoms),
              member(Atom, BodyAtoms), memberchk(Atom, Dead0) ),
            Killed0),
    append(Dead0, Killed0, Merged0), sort(Merged0, Merged),
    NextRounds is RoundsSoFar + 1,
    (   Merged == Dead0
    ->  Dead = Dead0, Rounds = NextRounds
    ;   overdelete_step(Rules, Model, Merged, NextRounds, Dead, Rounds) ).

rederive(Rules, Model, Base, Dead, Restored, Rounds) :-
    subtract(Model, Dead, Survivors0), sort(Survivors0, Survivors),
    rederive_step(Rules, Base, Dead, Survivors, 0, Restored, Rounds).

rederive_step(Rules, Base, Dead, Survivors, RoundsSoFar, Restored, Rounds) :-
    append(Base, Survivors, Visible0), sort(Visible0, Visible),
    findall(Head,
            ( member(Head, Dead),
              derivation(Rules, Visible, Head, _) ),
            Back0),
    sort(Back0, Back),
    append(Survivors, Back, Grown0), sort(Grown0, Grown),
    NextRounds is RoundsSoFar + 1,
    (   Grown == Survivors
    ->  Restored = Back, Rounds = NextRounds
    ;   rederive_step(Rules, Base, Dead, Grown, NextRounds, Restored, Rounds) ).

% ═══════════════════════════════════════════════════════════════════════════
% A1 FIXTURES: a scope whose support cone is a chain of Depth derived rels
% ═══════════════════════════════════════════════════════════════════════════
% open_scope is the minimal kernel's scope root row (an ordinary PROGRAM keyed
% Set rel). demand/4 is the forest's stored demand row. Both feed the SAME
% demanded/2 projection, so the cone below demanded/2 is byte-identical and the
% two models can be compared on one fixture.

chain_rules(Depth, kernel, Rules) :-
    chain_cone(Depth, Cone),
    Rules = [ (demanded(feed, Instance) <- open_scope(Instance, feed)) | Cone ].
chain_rules(Depth, forest, Rules) :-
    chain_cone(Depth, Cone),
    Rules = [ (demanded(Target, Salt) <- demand(_DemandId, _SubId, Target, Salt)) | Cone ].

chain_cone(Depth, Cone) :-
    numlist(1, Depth, Levels),
    maplist(chain_rule, Levels, Cone).

chain_rule(1, (view(1, Value) <- demanded(feed, _), base(Value))) :- !.
chain_rule(Level, (view(Level, Value) <- view(Below, Value))) :- Below is Level - 1.

chain_base(kernel, [ open_scope(1, feed), base(alpha) ]).
chain_base(forest, [ demand(1001, 1, feed, 1), base(alpha) ]).

% teardown = the scope root leaves. Kernel: retract the one program row.
% Forest: the 3-statement prefix range-DELETE removes sub/sub_path/demand.
% Either way the cone below demanded/2 is what has to move.
chain_after_teardown(_, [ base(alpha) ]).

plant_rounds(Model, Depth, Rounds) :-
    chain_rules(Depth, Model, Rules), chain_base(Model, Base),
    closure(Rules, Base, _, Rounds).

recompute_teardown_rounds(Model, Depth, Rounds) :-
    chain_rules(Depth, Model, Rules), chain_after_teardown(Model, Base),
    closure(Rules, Base, _, Rounds).

cascade_teardown_rounds(Model, Depth, Rounds) :-
    chain_rules(Depth, Model, Rules), chain_base(Model, Base),
    closure(Rules, Base, Level, _),
    append(Base, Level, Model0), sort(Model0, FullModel),
    scope_root_row(Model, Root),
    overdelete(Rules, FullModel, [Root], _, Rounds).

scope_root_row(kernel, open_scope(1, feed)).
scope_root_row(forest, demand(1001, 1, feed, 1)).

% the same chain with a SECOND independent support under it, which is what
% forces DRed's rederivation phase
chain_rules_with_alt(Depth, Rules) :-
    chain_rules(Depth, kernel, Base),
    append(Base, [ (view(1, Value) <- alt_base(Value)) ], Rules).

% nested SCOPES (not a cone): Depth scopes, each one joining its parent's row.
% This is derived_nesting from switch_flow section 7.2, generalized in depth.
nest_rules(Depth, Rules) :-
    numlist(1, Depth, Levels),
    maplist(nest_rule, Levels, Rules).

nest_rule(1, (live(1, Target) <- open_root(Target))) :- !.
nest_rule(Level, (live(Level, Target) <- live(Above, _), open_level(Level, Target))) :-
    Above is Level - 1.

nest_base(Depth, Base) :-
    numlist(2, Depth, Levels),
    maplist(nest_open_row, Levels, Opens),
    Base = [ open_root(root_target) | Opens ].

nest_open_row(Level, open_level(Level, target(Level))).

nest_cascade_rounds(Depth, Rounds) :-
    nest_rules(Depth, Rules), nest_base(Depth, Base),
    closure(Rules, Base, Level, _),
    append(Base, Level, Model0), sort(Model0, FullModel),
    overdelete(Rules, FullModel, [open_root(root_target)], _, Rounds).

% ═══════════════════════════════════════════════════════════════════════════
% A1 CHECKS
% ═══════════════════════════════════════════════════════════════════════════

check(plant_rounds_grow_linearly_with_cone_depth,
  ( maplist([Depth, Rounds]>>plant_rounds(kernel, Depth, Rounds),
            [2, 4, 8, 16], Observed),
    Observed == [4, 6, 10, 18] )).

% the whole ambiguity-9 question, answered: recompute teardown is O(1) rounds
% because the answer is EMPTY, so nothing has to be walked at all
check(recompute_teardown_is_one_round_at_every_cone_depth,
  ( maplist([Depth, Rounds]>>recompute_teardown_rounds(kernel, Depth, Rounds),
            [2, 4, 8, 16], Observed),
    Observed == [1, 1, 1, 1] )).

% under a DRed/counting lowering the same teardown is f(depth), and this is the
% cost switch_flow ambiguity 9 flagged and could not measure
check(cascade_teardown_rounds_are_a_function_of_cone_depth,
  ( maplist([Depth, Rounds]>>cascade_teardown_rounds(kernel, Depth, Rounds),
            [2, 4, 8, 16], Observed),
    Observed == [4, 6, 10, 18] )).

% THE ARITHMETIC THAT DECIDES A1: the forest's 3-statement range-DELETE covers
% sub/sub_path/demand and NOTHING BELOW demanded/2. Same cone, same rounds.
check(the_range_delete_never_covered_the_view_cone,
  ( maplist([Depth, Rounds]>>cascade_teardown_rounds(forest, Depth, Rounds),
            [2, 4, 8, 16], ForestRounds),
    maplist([Depth, Rounds]>>cascade_teardown_rounds(kernel, Depth, Rounds),
            [2, 4, 8, 16], KernelRounds),
    ForestRounds == KernelRounds )).

check(recompute_teardown_is_identical_in_both_models,
  ( maplist([Depth, Rounds]>>recompute_teardown_rounds(forest, Depth, Rounds),
            [2, 4, 8, 16], ForestRounds),
    maplist([Depth, Rounds]>>recompute_teardown_rounds(kernel, Depth, Rounds),
            [2, 4, 8, 16], KernelRounds),
    ForestRounds == KernelRounds )).

% DRed's second phase. A cone with an alternative support pays the walk twice.
check(dred_rederivation_walks_the_cone_a_second_time,
  ( chain_rules_with_alt(8, Rules),
    chain_base(kernel, Base0), append(Base0, [alt_base(alpha)], Base),
    closure(Rules, Base, Level, _),
    append(Base, Level, Model0), sort(Model0, FullModel),
    overdelete(Rules, FullModel, [open_scope(1, feed)], Dead, DeleteRounds),
    rederive(Rules, FullModel, [alt_base(alpha), base(alpha)], Dead,
             Restored, RederiveRounds),
    DeleteRounds == 10, RederiveRounds == 9,
    plant_rounds(kernel, 8, PlantRounds), PlantRounds == 10,
    DeleteRounds + RederiveRounds =:= 19,
    length(Restored, 8) )).

% the forest's ONLY real advantage, isolated and quantified: nested SCOPES.
% Kernel cascades one round per nesting level; the forest deletes the whole
% subtree with one prefix range scan.
check(the_forest_advantage_is_exactly_the_scope_nesting_depth,
  ( maplist([Depth, Rounds]>>nest_cascade_rounds(Depth, Rounds),
            [2, 4, 8, 16], Observed),
    Observed == [3, 5, 9, 17] )).

% the laziness gate in the minimal kernel has no stored demand row to look up,
% so switch_flow's demand_present/4 falls through to a full level closure per
% fill item. The forest short-circuits on one stored row.
check(the_kernel_laziness_gate_costs_a_closure_per_fill,
  ( plant_rounds(kernel, 8, GateRounds),
    GateRounds == 10,
    chain_base(forest, ForestBase),
    memberchk(demand(_, _, feed, _), ForestBase) )).

% ═══════════════════════════════════════════════════════════════════════════
% A2: WHERE THE STATE WENT. Forensics from the store alone.
% ═══════════════════════════════════════════════════════════════════════════
% The repo's self-diagnosis law says the trail must answer from disk. Two
% questions the forest answered from rows and the minimal kernel cannot.

two_supporters_program(
  [ (demanded(feed(alpha), 1) <- open_left(1)),
    (demanded(feed(alpha), 1) <- open_right(1)) ]).

check(two_scope_rules_producing_one_demand_row_are_unattributable,
  ( two_supporters_program(Rules),
    closure(Rules, [open_left(1), open_right(1)], BothLevel, _),
    closure(Rules, [open_left(1)], LeftOnlyLevel, _),
    BothLevel == LeftOnlyLevel,
    BothLevel == [demanded(feed(alpha), 1)] )).

% the forest kept one demand row PER subscriber, so the count and the owner are
% on disk; grading the row count is the whole difference
check(the_forest_kept_one_demand_row_per_supporter,
  ( ForestRows = [ demand(1001, 11, feed(alpha), 11),
                   demand(1002, 12, feed(alpha), 12) ],
    length(ForestRows, 2),
    maplist([demand(_, SubId, _, _), SubId]>>true, ForestRows, Owners),
    Owners == [11, 12] )).

% nesting in the minimal kernel is a JOIN in a rule body. Nothing in the store
% says pane_one is the parent of detail(item_a); the relation is rule text.
nested_scope_program(
  [ (live_detail(PaneId, detail(ItemId)) <- open_pane(PaneId, _), open_detail(PaneId, ItemId)),
    (demanded(Target, PaneId) <- live_detail(PaneId, Target)) ]).

check(the_store_cannot_name_the_parent_of_a_nested_scope,
  ( nested_scope_program(Rules),
    closure(Rules, [open_pane(pane_one, item_list), open_detail(pane_one, item_a)],
            Level, _),
    \+ ( member(Row, Level), functor(Row, sub_path, _) ),
    \+ ( member(Row, Level), functor(Row, parent_of, _) ),
    memberchk(demanded(detail(item_a), pane_one), Level) )).

% ═══════════════════════════════════════════════════════════════════════════
% A2b: THE STRUCTURAL GUARANTEE THAT WAS TRADED AWAY
% ═══════════════════════════════════════════════════════════════════════════
% In the forest a child scope is planted UNDER its parent's materialized path,
% so prefix teardown covers it by construction. In the minimal kernel the parent
% link is one body atom, and deleting that atom leaks the inner scope forever.
% Verified against the live lab as well (probe kept here so it survives the lab).

leaky_nesting_program(
  [ (live_detail(PaneId, detail(ItemId)) <- open_detail(PaneId, ItemId)),
    (demanded(Target, PaneId) <- live_detail(PaneId, Target)) ]).

check(dropping_one_body_atom_leaks_the_inner_scope_forever,
  ( nested_scope_program(SoundRules),
    leaky_nesting_program(LeakyRules),
    Closed = [ open_detail(pane_one, item_a) ],
    closure(SoundRules, Closed, SoundLevel, _),
    closure(LeakyRules, Closed, LeakyLevel, _),
    SoundLevel == [],
    memberchk(demanded(detail(item_a), pane_one), LeakyLevel) )).

check(the_leak_is_one_atom_of_difference_and_no_check_sees_it,
  ( nested_scope_program([SoundRule | _]),
    leaky_nesting_program([LeakyRule | _]),
    SoundRule = (SoundHead <- SoundBody),
    LeakyRule = (LeakyHead <- LeakyBody),
    SoundHead =@= LeakyHead,
    body_atoms(SoundBody, SoundAtoms), body_atoms(LeakyBody, LeakyAtoms),
    length(SoundAtoms, 2), length(LeakyAtoms, 1) )).

% the forest cannot express the leak: a plant is parented by construction
forest_plant(ParentPath, ChildId, ChildPath) :- append(ParentPath, [ChildId], ChildPath).

check(the_forest_cannot_express_the_leak,
  ( forest_plant([1], 1001, Path),
    Path == [1, 1001],
    append([1], _, Path) )).

% ═══════════════════════════════════════════════════════════════════════════
% A3: IS THE STATIC LAW DECIDABLE, AND WHAT DOES IT GET WRONG
% ═══════════════════════════════════════════════════════════════════════════
% Law under test (switch_flow 7.4): "self-completion may not level-rule over
% rows produced under its own scope". As implemented in the lab it is exactly
% predicate-level stratification, which IS decidable. Both failure directions
% below are consequences of that reduction.

rule_edges(Rules, Edges) :- maplist(rule_edge_list, Rules, Nested), append(Nested, Edges).

rule_edge_list((Head <- Body), Edges) :-
    functor(Head, HeadName, HeadArity),
    positive_refs(Body, Positive), negative_refs(Body, Negative),
    maplist([Ref, edge(HeadName/HeadArity, Ref, pos)]>>true, Positive, PosEdges),
    maplist([Ref, edge(HeadName/HeadArity, Ref, neg)]>>true, Negative, NegEdges),
    append(PosEdges, NegEdges, Edges).

positive_refs(Body, Refs) :-
    body_atoms(Body, Atoms),
    maplist([Atom, Name/Arity]>>functor(Atom, Name, Arity), Atoms, Refs).

negative_refs((Left, Right), Refs) :- !,
    negative_refs(Left, LeftRefs), negative_refs(Right, RightRefs),
    append(LeftRefs, RightRefs, Refs).
negative_refs(not(Goal), [Name/Arity]) :- !, functor(Goal, Name, Arity).
negative_refs(_, []).

reaches(Edges, From, To) :- reaches_walk(Edges, From, To, [From]).

reaches_walk(Edges, From, To, _) :- memberchk(edge(From, To, _), Edges).
reaches_walk(Edges, From, To, Seen) :-
    member(edge(From, Middle, _), Edges),
    \+ memberchk(Middle, Seen),
    reaches_walk(Edges, Middle, To, [Middle | Seen]).

stratified(Rules) :-
    rule_edges(Rules, Edges),
    \+ ( member(edge(Head, Negated, neg), Edges), reaches(Edges, Negated, Head) ).

% PROGRAM A. The completion condition IS a level rule over rows produced under
% the scope. The negative cycle closes through the DEMAND GATE, which is a bind
% edge, not a rule edge, so a rule-graph stratification check cannot see it.
unsound_fork_rules(
  [ (fork_closed(Session) <- result_a(_), result_b(_), open_fork(Session, _)),
    (live_fork(Session, Target) <- open_fork(Session, Target), not(fork_closed(Session))),
    (demanded(Target, Session) <- live_fork(Session, Target)) ]).

% the edge the bind layer really has: result_a only exists because it was
% demanded. Written as a rule it closes the cycle and the check fires.
demand_gate_edge((result_a(alpha) <- demanded(arm_target, _))).

check(rule_graph_stratification_accepts_the_unsound_self_completion,
  ( unsound_fork_rules(Rules), stratified(Rules) )).

check(the_demand_gate_edge_is_what_makes_the_cycle_visible,
  ( unsound_fork_rules(Rules), demand_gate_edge(GateRule),
    append(Rules, [GateRule], WithGate),
    \+ stratified(WithGate) )).

% PROGRAM B. Generation-indexed self-completion: scope generation Gen completes
% on a STRICTLY EARLIER generation's result. Locally stratified (the generation
% strictly decreases around every cycle, so the instance graph is well-founded),
% and the predicate-level check refuses it anyway.
generation_rules(
  [ (done(Session, Gen) <- result(Session, Prev), Prev < Gen),
    (live(Session, Gen) <- open_gen(Session, Gen), not(done(Session, Gen))),
    (result(Session, Gen) <- demanded(target(Session), Gen), row(Session)),
    (demanded(target(Session), Gen) <- live(Session, Gen)) ]).

% evaluate generation by generation, which IS the local stratification order
generation_model(Rules, Base, MaxGen, Model) :-
    numlist(0, MaxGen, Gens),
    foldl(generation_stratum(Rules, Base), Gens, [], Model).

generation_stratum(Rules, Base, Gen, Known, Next) :-
    append(Base, Known, Visible0), sort(Visible0, Visible),
    closure(Rules, Visible, Level, _),
    include(gen_at_most(Gen), Level, Allowed),
    append(Known, Allowed, Merged0), sort(Merged0, Next).

gen_at_most(Gen, Row) :-
    Row =.. [_ | Arguments],
    forall(( member(Argument, Arguments), integer(Argument) ), Argument =< Gen).

check(predicate_stratification_refuses_the_generation_indexed_program,
  ( generation_rules(Rules), \+ stratified(Rules) )).

check(the_generation_indexed_program_has_a_settled_model_anyway,
  ( generation_rules(Rules),
    generation_model(Rules, [open_gen(session_one, 0), open_gen(session_one, 1),
                             row(session_one)], 1, Model),
    memberchk(live(session_one, 0), Model),
    memberchk(demanded(target(session_one), 0), Model) )).

% the law is decidable exactly because it reduces to stratification, and
% stratification is a check the engine already owes for negation
check(the_new_law_is_the_stratification_check_the_engine_already_owes,
  ( unsound_fork_rules(Rules), rule_edges(Rules, Edges),
    memberchk(edge(live_fork/2, fork_closed/1, neg), Edges) )).

% ═══════════════════════════════════════════════════════════════════════════
% A4: SILENT SERIALIZATION. Is there ANY runtime artifact of a losing plant?
% ═══════════════════════════════════════════════════════════════════════════
% Two keys fight over one flattening slot. Boundary deltas are a set diff of the
% tick's start and end states (r7), so a row written and replaced INSIDE one
% tick emits neither a + nor a -.

keyed_replace([], Row, [Row]).
keyed_replace([Old | Rest], Row, Store) :-
    (   same_key(Old, Row)
    ->  Store = [Row | Rest]
    ;   keyed_replace(Rest, Row, Tail), Store = [Old | Tail] ).

same_key(Left, Right) :-
    Left =.. [Name, Key | _], Right =.. [Name, Key | _].

boundary_delta(Before, After, Deltas) :-
    findall(-Row, ( member(Row, Before), \+ memberchk(Row, After) ), Removals),
    findall(+Row, ( member(Row, After), \+ memberchk(Row, Before) ), Additions),
    append(Removals, Additions, Deltas).

check(a_losing_keyed_plant_emits_no_boundary_delta,
  ( Before = [],
    keyed_replace(Before, open_tab(session_one, tab_a), Mid),
    keyed_replace(Mid, open_tab(session_one, tab_b), After),
    Mid == [open_tab(session_one, tab_a)],
    boundary_delta(Before, After, Deltas),
    Deltas == [+open_tab(session_one, tab_b)],
    \+ memberchk(+open_tab(session_one, tab_a), Deltas),
    \+ memberchk(-open_tab(session_one, tab_a), Deltas) )).

% the forest is NOT better here: switch_flow's own
% one_parent_scope_is_one_flattening_slot grades scope_birth_ticks == [], an
% invisible scope. What the forest does leave is a GAP in the dense id sequence.
forest_plant_sequence(Targets, StartId, FinalId, BirthIds) :-
    foldl(forest_plant_one, Targets, StartId-[], FinalId-BirthIds0),
    reverse(BirthIds0, BirthIds).

forest_plant_one(_Target, Id0-Births, Id-[Id | Births]) :- Id is Id0 + 1.

check(the_id_sequence_gap_is_the_forests_only_artifact_of_a_losing_plant,
  ( forest_plant_sequence([fetch_of(gh_repos), fetch_of(gh_issues)], 1000,
                          FinalId, BirthIds),
    FinalId == 1002, BirthIds == [1001, 1002],
    SurvivingSubs = [1002],
    length(BirthIds, Planted), length(SurvivingSubs, Alive),
    Planted - Alive =:= 1 )).

check(the_minimal_kernel_has_no_sequence_so_it_has_no_artifact_at_all,
  ( Before = [],
    keyed_replace(Before, open_tab(session_one, tab_a), Mid),
    keyed_replace(Mid, open_tab(session_one, tab_b), After),
    boundary_delta(Before, After, Deltas),
    length(Deltas, 1),
    length(After, 1) )).

% ═══════════════════════════════════════════════════════════════════════════
% A5: THE SUGAR ROUND TRIP. Zero rels per KERNEL, how many per PROGRAM?
% ═══════════════════════════════════════════════════════════════════════════
% Desugaring of `switch_map` per switch_flow 7.7: one keyed rel decl, one edge
% rule that writes the scope row, one level rule that projects demanded/2.

desugar_site(SiteIndex, site(RelName, keyed(RelName/2, KeyPositions), [PlantRule, DemandRule])) :-
    atom_concat(open_scope_, SiteIndex, RelName),
    KeyPositions = [1],
    PlantHead =.. [RelName, key, target],
    PlantRule = (PlantHead <- trigger(SiteIndex)),
    DemandBody =.. [RelName, key, target],
    DemandRule = (demanded(target, key) <- DemandBody).

check(desugaring_fifty_switch_sites_mints_fifty_rels_and_a_hundred_rules,
  ( numlist(1, 50, Indices),
    maplist(desugar_site, Indices, Sites),
    maplist([site(RelName, _, _), RelName]>>true, Sites, RelNames),
    sort(RelNames, UniqueRels), length(UniqueRels, 50),
    maplist([site(_, _, Rules), Rules]>>true, Sites, RuleLists),
    append(RuleLists, AllRules), length(AllRules, 100),
    maplist([site(_, Decl, _), Decl]>>true, Sites, Decls), length(Decls, 50) )).

% could sugar share ONE scope rel across sites? Only with a site discriminator
% column: keying on the outer key alone makes two sites collide, and the second
% site's scope silently replaces the first's.
check(sharing_one_scope_rel_keyed_by_the_outer_key_alone_collides_across_sites,
  ( keyed_replace([], scope(session_one, route_target), Mid),
    keyed_replace(Mid, scope(session_one, detail_target), After),
    After == [scope(session_one, detail_target)],
    length(After, 1) )).

check(the_site_column_restores_the_forests_scope_id_by_another_name,
  ( keyed_replace([], scope(site_one, session_one, route_target), Mid),
    keyed_replace(Mid, scope(site_two, session_one, detail_target), After),
    length(After, 2),
    functor(scope(site_one, session_one, route_target), _, SharedArity),
    functor(sub(1001, 1, detail_target), _, ForestArity),
    SharedArity == ForestArity )).

% the four policy words do not disappear; their STORAGE moves into the key
% declaration, and the surface still needs four distinct desugarings
policy_desugaring(switch,  keyed(open_tab/2, [1]),    true).
policy_desugaring(merge,   keyed(open_tab/2, [1, 2]), true).
policy_desugaring(exhaust, keyed(open_tab/2, [1]),    not(live_tab(_))).
policy_desugaring(concat,  keyed(open_tab/2, [1]),    replay(pending_tab/2)).

check(the_policy_word_survives_desugaring_as_four_distinct_expansions,
  ( maplist([Policy, Policy-Key-Guard]>>policy_desugaring(Policy, Key, Guard),
            [switch, merge, exhaust, concat], Expansions),
    sort(Expansions, Unique), length(Unique, 4) )).

check(concat_is_the_one_policy_whose_expansion_needs_a_second_program_rel,
  ( policy_desugaring(concat, _, replay(PendingRef)),
    PendingRef == pending_tab/2,
    maplist([Policy, Guard]>>policy_desugaring(Policy, _, Guard),
            [switch, merge, exhaust], Guards),
    \+ ( member(Guard, Guards), Guard = replay(_) ) )).

% ═══════════════════════════════════════════════════════════════════════════
go :- run(check).

report :-
    forall(member(Depth, [2, 4, 8, 16, 32]),
           ( plant_rounds(kernel, Depth, Plant),
             recompute_teardown_rounds(kernel, Depth, Recompute),
             cascade_teardown_rounds(kernel, Depth, Cascade),
             nest_cascade_rounds(Depth, Nest),
             format("depth ~w  plant ~w  recompute-teardown ~w  cascade-teardown ~w  nested-cascade ~w~n",
                    [Depth, Plant, Recompute, Cascade, Nest]) )).
