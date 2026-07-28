% level_eval.pl : stratified level evaluation with aggregates.
% Rules group by head stratum (negated and aggregated rels strictly below
% their consumers; not_stratified on cycles); q7 bag multiplicity; q9
% reserved head forms incl. the json arm.

:- module(level_eval,
          [ split_rules/4, level_closure/5, aggregate_head/3 ]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(library(pairs)).
:- use_module(body).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

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

% Stratified evaluation (defect found by the timeless_rail promotion: a joint
% fixpoint let not/1 over a DERIVED rel read an incomplete set and permanently
% admit wrong rows). Rules group by head stratum; a negated or aggregated rel
% must sit strictly below its consumer, so by the time a stratum runs, every
% rel it negates or aggregates is complete.
level_closure(PlainLevel, AggRules, Base, Tick, Level) :-
    append(PlainLevel, AggRules, LevelRules),
    stratify_level_rules(LevelRules, Strata),
    eval_strata(Strata, Base, Tick, [], Level).

eval_strata([], _, _, Acc, Level) :- sort(Acc, Level).
eval_strata([Group | Rest], Base, Tick, Acc0, Level) :-
    append(Base, Acc0, Lower),
    findall(Rule, ( member(Rule, Group), Rule = (Head <- _), aggregate_head(Head, _, _) ),
            GroupAgg),
    findall(Rule, ( member(Rule, Group), Rule = (Head <- _), \+ aggregate_head(Head, _, _) ),
            GroupPlain),
    findall(Row, ( member(Rule, GroupAgg), agg_rule_rows(Rule, Lower, Tick, Row) ), AggRows),
    append(Lower, AggRows, StratumBase),
    plain_fixpoint(GroupPlain, StratumBase, Tick, [], PlainRows),
    append([Acc0, AggRows, PlainRows], Acc),
    eval_strata(Rest, Base, Tick, Acc, Level).

% ── stratum assignment ──────────────────────────────────────────────────────
% S(head) >= S(body rel) for a positive read; strictly greater for a rel under
% not/1 or feeding an aggregate head. Only level-rule heads are derived; every
% other rel (facts, stored, edge-headed) is stratum 0. A program that cannot
% stabilize (negation or aggregation through a cycle) throws not_stratified.

stratify_level_rules(LevelRules, Strata) :-
    findall(Ref, ( member((Head <- _), LevelRules), rel_ref(Head, Ref) ), DerivedRefs0),
    sort(DerivedRefs0, DerivedRefs),
    findall(HeadRef-constraint(BodyRef, Gap),
            ( member((Head <- Body), LevelRules),
              rel_ref(Head, HeadRef),
              rule_body_constraint(Head, Body, BodyRef, Gap),
              memberchk(BodyRef, DerivedRefs) ),
            Constraints),
    length(DerivedRefs, DerivedCount),
    Cap is DerivedCount + 1,
    findall(Ref-0, member(Ref, DerivedRefs), Strata0),
    relax_strata(Constraints, Cap, Strata0, StrataMap),
    findall(Number-Rule,
            ( member(Rule, LevelRules), Rule = (Head <- _),
              rel_ref(Head, Ref), memberchk(Ref-Number, StrataMap) ),
            Numbered),
    keysort(Numbered, Sorted),
    group_pairs_by_key(Sorted, Grouped),
    pairs_values(Grouped, Strata).

rule_body_constraint(Head, Body, BodyRef, Gap) :-
    (   aggregate_head(Head, _, _)
    ->  goal_rel_refs(Body, PosRefs, NegRefs),
        append(PosRefs, NegRefs, AllRefs),
        member(BodyRef, AllRefs), Gap = 1
    ;   goal_rel_refs(Body, PosRefs, NegRefs),
        (   member(BodyRef, PosRefs), Gap = 0
        ;   member(BodyRef, NegRefs), Gap = 1 )
    ).

goal_rel_refs((Left, Right), Pos, Neg) :- !,
    goal_rel_refs(Left, LeftPos, LeftNeg),
    goal_rel_refs(Right, RightPos, RightNeg),
    append(LeftPos, RightPos, Pos), append(LeftNeg, RightNeg, Neg).
goal_rel_refs(not(Goal), [], Neg) :- !,
    goal_rel_refs(Goal, InnerPos, InnerNeg),
    append(InnerPos, InnerNeg, Neg).
goal_rel_refs(latest(Atom), [Ref], []) :- !, rel_ref(Atom, Ref).
goal_rel_refs(departed(_), [], []) :- !.
goal_rel_refs(pre(_), [], []) :- !.
goal_rel_refs(now(_), [], []) :- !.
goal_rel_refs(true, [], []) :- !.
goal_rel_refs(_ := _, [], []) :- !.
goal_rel_refs(_ is _, [], []) :- !.
goal_rel_refs(decode(_, _), [], []) :- !.
goal_rel_refs(json_each(_, _), [], []) :- !.
goal_rel_refs(Goal, [], []) :- comparison_goal(Goal), !.
goal_rel_refs(Atom, [Ref], []) :- rel_ref(Atom, Ref).

relax_strata(Constraints, Cap, Strata0, Strata) :-
    findall(changed,
            ( member(HeadRef-constraint(BodyRef, Gap), Constraints),
              memberchk(HeadRef-HeadStratum, Strata0),
              memberchk(BodyRef-BodyStratum, Strata0),
              HeadStratum < BodyStratum + Gap ),
            Changes),
    (   Changes == []
    ->  Strata = Strata0
    ;   findall(Ref-Number,
                ( member(Ref-Current, Strata0),
                  findall(Needed,
                          ( member(Ref-constraint(BodyRef, Gap), Constraints),
                            memberchk(BodyRef-BodyStratum, Strata0),
                            Needed is BodyStratum + Gap ),
                          Neededs),
                  max_list([Current | Neededs], Number) ),
                Strata1),
        forall(member(_-Number, Strata1),
               ( Number =< Cap -> true ; throw(not_stratified) )),
        relax_strata(Constraints, Cap, Strata1, Strata)
    ).

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
