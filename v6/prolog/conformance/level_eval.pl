% level_eval.pl : stratified level evaluation with aggregates.
% Rules group by head stratum (negated and aggregated rels strictly below
% their consumers; not_stratified on cycles); q7 bag multiplicity; q9
% reserved head forms incl. the json arm.

:- module(level_eval,
          [ split_rules/4, level_closure/6,
            % Its not/1 clause carries an ordering contract the shared walker
            % deliberately does not reproduce.
            goal_rel_refs/3,
            % The boundary read of a list column, engine.pl's last pass.
            list_boundary_rows/4, list_boundary_deltas/5 ]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(library(pairs)).
:- use_module(body).
:- use_module('../compile/registry', [surface_for_term/6]).
:- use_module('../0_type_plane', [canonical_json_text/2]).
:- use_module('../next/1_expand/0_generic_expand', [canonical_type_name/2]).

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

% The aggregate inventory comes from registry.pl, not a local list.
%
% Status is not consulted because the reference engine executes aggregate
% forms that the compiler refuses.
classify_head_arg(Arg, agg(json_object, KeyExpr-ValueExpr)) :-
    nonvar(Arg), Arg = json_object(KeyExpr, ValueExpr), !.
classify_head_arg(Arg, agg(json_group_array, ValueExpr)) :-
    nonvar(Arg),
    surface_for_term(Arg, json_group_array/1, aggregate, no_refs, head(_), live),
    arg(1, Arg, ValueExpr), !.
classify_head_arg(Arg, agg(json_group_array_ordered, ValueExpr-OrdinalExpr)) :-
    nonvar(Arg),
    surface_for_term(Arg, json_group_array/2, aggregate, no_refs, head(_), live),
    arg(1, Arg, ValueExpr), arg(2, Arg, OrdinalExpr), !.
classify_head_arg(Arg, agg(group_concat(','), ValueExpr)) :-
    nonvar(Arg),
    surface_for_term(Arg, group_concat/1, aggregate, no_refs, head(_), live),
    arg(1, Arg, ValueExpr), !.
classify_head_arg(Arg, agg(group_concat(Sep), ValueExpr)) :-
    nonvar(Arg),
    surface_for_term(Arg, group_concat/2, aggregate, no_refs, head(_), live),
    arg(1, Arg, ValueExpr), arg(2, Arg, Sep), !.
classify_head_arg(Arg, agg(group_concat_ordered(Sep), ValueExpr-OrdinalExpr)) :-
    nonvar(Arg),
    surface_for_term(Arg, group_concat/3, aggregate, no_refs, head(_), live),
    arg(1, Arg, ValueExpr), arg(2, Arg, Sep), arg(3, Arg, OrdinalExpr), !.
classify_head_arg(Arg, agg(Kind, Expr)) :-
    nonvar(Arg),
    surface_for_term(Arg, Kind/1, aggregate, _, head(_), _), !,
    arg(1, Arg, Expr).
classify_head_arg(Arg, plain(Arg)).

split_rules(Rules, AggRules, PlainLevel, EdgeRules) :-
    findall(Rule, ( member(Rule, Rules), Rule = (Head <- _), aggregate_head(Head, _, _) ),
            AggRules),
    findall(Rule, ( member(Rule, Rules), Rule = (Head <- _), \+ aggregate_head(Head, _, _) ),
            PlainLevel),
    findall(Rule, ( member(Rule, Rules), Rule = (_ <+ _) ), EdgeRules).

% A negated or aggregated relation must sit strictly below its consumer, so a
% stratum reads complete inputs.
level_closure(Decls, PlainLevel, AggRules, Base, Tick, Level) :-
    append(PlainLevel, AggRules, LevelRules),
    stratify_level_rules(LevelRules, Strata),
    eval_strata(Strata, Decls, Base, Tick, [], Level).

eval_strata([], _, _, _, Acc, Level) :- sort(Acc, Level).
eval_strata([Group | Rest], Plane, Base, Tick, Acc0, Level) :-
    append(Base, Acc0, Lower),
    findall(Rule, ( member(Rule, Group), Rule = (Head <- _), aggregate_head(Head, _, _) ),
            GroupAgg),
    findall(Rule, ( member(Rule, Group), Rule = (Head <- _), \+ aggregate_head(Head, _, _) ),
            GroupPlain),
    findall(Row, ( member(Rule, GroupAgg), agg_rule_rows(Rule, Lower, Tick, Row) ), AggRows),
    append(Lower, AggRows, StratumBase),
    plain_fixpoint(Plane, GroupPlain, StratumBase, Tick, [], PlainRows),
    append([Acc0, AggRows, PlainRows], Acc),
    eval_strata(Rest, Plane, Base, Tick, Acc, Level).

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
goal_rel_refs(Goal, Pos, Neg) :-
    surface_for_term(Goal, _, _, splice_bare, _, _),
    !,
    Goal =.. [_ | Atoms],
    goal_list_rel_refs(Atoms, Pos, Neg).
goal_rel_refs(latest(Atom), [Ref], []) :- !, rel_ref(Atom, Ref).
goal_rel_refs(finalize(_), [], []) :- !.
goal_rel_refs(pre(_), [], []) :- !.
goal_rel_refs(pre(_, _), [], []) :- !.
goal_rel_refs(now(_), [], []) :- !.
goal_rel_refs(true, [], []) :- !.
goal_rel_refs(_ := _, [], []) :- !.
goal_rel_refs(_ is _, [], []) :- !.
goal_rel_refs(decode(_, _), [], []) :- !.
goal_rel_refs(json_each(_, _), [], []) :- !.
goal_rel_refs(Goal, [], []) :- comparison_goal(Goal), !.
goal_rel_refs(Atom, [Ref], []) :- rel_ref(Atom, Ref).

goal_list_rel_refs([], [], []).
goal_list_rel_refs([Goal | Rest], Pos, Neg) :-
    goal_rel_refs(Goal, GoalPos, GoalNeg),
    goal_list_rel_refs(Rest, RestPos, RestNeg),
    append(GoalPos, RestPos, Pos),
    append(GoalNeg, RestNeg, Neg).

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

% A head whose measure grows every round (`Next := Value + 1`) has no finite
% least model, so `Merged == Known0` never holds. Mirrors engine.pl's
% drain_cap/1: bounded, loud, never a truncated answer. The round is a full
% re-derive over the whole known set, so the wall is quadratic in the cap and
% the cap buys nothing above the deepest converging level in the corpus.
level_round_cap(50).

plain_fixpoint(Plane, PlainLevel, Base, Tick, Known0, Level) :-
    rows_index(Base, BaseIndex),
    plain_fixpoint_(Plane, PlainLevel, BaseIndex, Tick, 0, Known0, Level).

plain_fixpoint_(Plane, PlainLevel, BaseIndex, Tick, Round, Known0, Level) :-
    level_round_cap(Cap),
    (   Round >= Cap
    ->  findall(Ref, ( member((Head <- _), PlainLevel), rel_ref(Head, Ref) ), Refs0),
        sort(Refs0, Refs),
        throw(diverging_measure_recursion(Refs, Cap))
    ;   true
    ),
    findall(EvaluatedHead,
            ( member((Head <- Body), PlainLevel),
              solve(Body, ctx(rows(BaseIndex, Known0), [], Tick)),
              eval_head(Head, EvaluatedHead) ),
            Derived),
    mint_heads(Plane, Derived, Heads, MintedRows),
    append([Known0, Heads, MintedRows], Merged0),
    sort(Merged0, Merged),
    ( Merged == Known0 -> Level = Known0
    ; NextRound is Round + 1,
      plain_fixpoint_(Plane, PlainLevel, BaseIndex, Tick, NextRound, Merged, Level) ).

% ── list(T) in head position: the value IS the interned list id ─────────────
% Canonical order, matching the emitted door's ORDER BY <array text>: every
% content text new to this batch is minted in sorted order BEFORE the
% substitution walk, so walk order can never leak into which id lands where.

mint_heads(Decls, Derived, Heads, Rows) :-
    findall(ContentText-Elements,
            batch_list_content_text(Decls, Derived, ContentText, Elements),
            ContentTexts),
    sort(ContentTexts, DistinctContentTexts),
    forall(member(ContentText-Elements, DistinctContentTexts),
           list_mint_id(ContentText, Elements, _)),
    mint_heads_(Decls, Derived, Heads, Rows).

batch_list_content_text(Decls, Derived, ContentText, Elements) :-
    member(Head, Derived),
    head_list_content_text(Decls, Head, ContentText, Elements).

head_list_content_text(Decls, Head, ContentText, Value) :-
    Head =.. [Name | Values],
    length(Values, Arity),
    list_column_element_types(Decls, Name/Arity, ElementTypes),
    nth0(Index, ElementTypes, Element), Element \== none,
    nth0(Index, Values, Value), is_list(Value),
    canonical_json_text(Value, ContentText).

mint_heads_(_, [], [], []).
mint_heads_(Decls, [Derived | Rest], [Head | Heads], Rows) :-
    mint_head(Decls, Derived, Head, HereRows),
    mint_heads_(Decls, Rest, Heads, MoreRows),
    append(HereRows, MoreRows, Rows).

mint_head(Decls, Derived, Head, Rows) :-
    Derived =.. [Name | Values],
    length(Values, Arity),
    (   list_column_element_types(Decls, Name/Arity, ElementTypes)
    ->  mint_values(ElementTypes, Values, Minted, [], Rows),
        Head =.. [Name | Minted]
    ;   Head = Derived, Rows = []
    ).

% none at a position whose column is not a list; the arity check keeps a rel
% whose columns are undeclared out of the walk entirely.
list_column_element_types(Decls, Name/Arity, ElementTypes) :-
    findall(Column, member(col_type(Name/Arity, Column, _), Decls), Columns),
    length(Columns, Arity),
    findall(Element,
            ( member(Column, Columns),
              (   memberchk(list_column(Name/Arity, Column, list(Found)), Decls)
              ->  Element = Found
              ;   Element = none
              ) ),
            ElementTypes),
    once(( member(Element0, ElementTypes), Element0 \== none )).

mint_values([], [], [], Rows, Rows).
mint_values([Element | Elements], [Value | Values], [Minted | Rest], Rows0, Rows) :-
    (   Element \== none, is_list(Value)
    ->  mint_list_value(Element, Value, Minted, HereRows),
        append(Rows0, HereRows, Rows1)
    ;   Minted = Value, Rows1 = Rows0
    ),
    mint_values(Elements, Values, Rest, Rows1, Rows).

mint_list_value(ElementType, Elements, Id, [EntityRow | MemberRows]) :-
    canonical_json_text(Elements, ContentText),
    list_mint_id(ContentText, Elements, Id),
    canonical_type_name(list(ElementType), EntityName),
    atomic_list_concat([EntityName, member], '__', MemberName),
    EntityRow =.. [EntityName, ContentText],
    findall(MemberRow,
            ( nth0(Index, Elements, Element),
              MemberRow =.. [MemberName, Id, Index, Element] ),
            MemberRows).

% ── the boundary read of a list column ─────────────────────────────────────
% Storage keeps the interned id, the same way a ref column stores an "__id";
% what crosses is the ordered member values, which is what the emitted
% `__list_<entity>` view aggregates out of the same member rel.

list_boundary_rows(Decls, Rules, Rows0, Rows) :-
    list_positions(Decls, Rules, Positions),
    (   Positions == []
    ->  Rows = Rows0
    ;   maplist(list_boundary_row(Positions, Rows0), Rows0, Rows)
    ).

list_boundary_deltas(Decls, Rules, Visible, Deltas0, Deltas) :-
    list_positions(Decls, Rules, Positions),
    (   Positions == []
    ->  Deltas = Deltas0
    ;   maplist(list_boundary_delta(Positions, Visible), Deltas0, Deltas)
    ).

% A DERIVED rel carries no col_type decl, so its list-ness comes from the body
% atom its head variable reads, the same propagation the relplan performs.
list_positions(Decls, Rules, Positions) :-
    declared_list_positions(Decls, Seed),
    (   Seed == []
    ->  Positions = []
    ;   close_list_positions(Rules, Seed, Positions)
    ).

declared_list_positions(Decls, Positions) :-
    findall(Ref-Index-Element,
            ( member(list_column(Ref, Column, list(Element)), Decls),
              findall(Declared, member(col_type(Ref, Declared, _), Decls),
                      Columns),
              nth0(Index, Columns, Column) ),
            Positions0),
    sort(Positions0, Positions).

close_list_positions(Rules, Positions0, Positions) :-
    findall(HeadRef-HeadIndex-Element,
            ( member(Rule, Rules),
              rule_head_body(Rule, Head, Body),
              carried_list_position(Positions0, Head, Body, HeadRef, HeadIndex,
                                    Element) ),
            Carried),
    append(Positions0, Carried, Merged0),
    sort(Merged0, Merged),
    (   Merged == Positions0
    ->  Positions = Positions0
    ;   close_list_positions(Rules, Merged, Positions)
    ).

rule_head_body((Head <- Body), Head, Body).
rule_head_body((Head <+ Body), Head, Body).

carried_list_position(Positions, Head, Body, HeadRef, HeadIndex, Element) :-
    Head =.. [HeadName | HeadArgs],
    length(HeadArgs, HeadArity),
    HeadRef = HeadName/HeadArity,
    nth0(HeadIndex, HeadArgs, HeadArg),
    var(HeadArg),
    body_conjunct(Body, Conjunct),
    Conjunct =.. [BodyName | BodyArgs],
    length(BodyArgs, BodyArity),
    member(BodyName/BodyArity-BodyIndex-Element, Positions),
    nth0(BodyIndex, BodyArgs, BodyArg),
    BodyArg == HeadArg.

body_conjunct((Left, Right), Conjunct) :- !,
    ( body_conjunct(Left, Conjunct) ; body_conjunct(Right, Conjunct) ).
body_conjunct(Goal, Goal) :- compound(Goal).

list_boundary_delta(Positions, Visible, Signed0, Signed) :-
    (   Signed0 = +Row0
    ->  list_boundary_row(Positions, Visible, Row0, Row), Signed = +Row
    ;   Signed0 = -Row0
    ->  list_boundary_row(Positions, Visible, Row0, Row), Signed = -Row
    ;   Signed = Signed0
    ).

list_boundary_row(Positions, Visible, Row0, Row) :-
    Row0 =.. [Name | Values],
    length(Values, Arity),
    (   memberchk(Name/Arity-_-_, Positions)
    ->  findall(Rendered,
                ( nth0(Index, Values, Value),
                  (   memberchk(Name/Arity-Index-Element, Positions)
                  ->  list_boundary_value(Visible, Element, Value, Rendered)
                  ;   Rendered = Value ) ),
                RenderedValues),
        Row =.. [Name | RenderedValues]
    ;   Row = Row0
    ).

list_boundary_value(Visible, Element, Id, Elements) :-
    integer(Id),
    list_member_values(Visible, Element, Id, Raw),
    !,
    maplist(list_boundary_element(Visible, Element), Raw, Elements).
list_boundary_value(_, _, Value, Value).

list_member_values(Visible, Element, Id, Values) :-
    canonical_type_name(list(Element), EntityName),
    atomic_list_concat([EntityName, member], '__', MemberName),
    findall(Index-Value,
            ( member(Row, Visible), Row =.. [MemberName, Id, Index, Value] ),
            Pairs),
    keysort(Pairs, Sorted),
    pairs_values(Sorted, Values).

list_boundary_element(Visible, list(Inner), Value, Rendered) :- !,
    list_boundary_value(Visible, Inner, Value, Rendered).
list_boundary_element(_, _, Value, Value).

agg_rule_rows((Head <- Body), Visible, Tick, Row) :-
    aggregate_head(Head, Template, Ref),
    Ref = Name/_,
    findall(Contribution,
            ( solve(Body, ctx(Visible, [], Tick)),
              maplist(head_arg_value, Template, Contribution) ),
            Bag),
    Bag \== [],
    % One pass. Re-running group_key/3 over the whole bag once per key made
    % this O(bag x keys); keysort/2 is stable so a group keeps the bag's order,
    % and group_pairs_by_key/2 emits the keys in the same ascending order sort/2
    % gave.
    findall(GroupKey-Solution,
            ( member(Solution, Bag), group_key(Template, Solution, GroupKey) ),
            Keyed),
    keysort(Keyed, KeyedSorted),
    group_pairs_by_key(KeyedSorted, Groups),
    member(GroupKey-Group, Groups),
    aggregate_args(Template, Group, Args),
    Row =.. [Name | Args].

head_arg_value(plain(Expr), value(Value)) :- eval_expr(Expr, Value).
head_arg_value(agg(json_object, KeyExpr-ValueExpr), contrib(Key-Value)) :- !,
    eval_expr(KeyExpr, Key), eval_expr(ValueExpr, Value).
head_arg_value(agg(json_group_array_ordered, ValueExpr-OrdinalExpr),
               contrib(Ordinal-Value)) :- !,
    eval_expr(ValueExpr, Value), eval_expr(OrdinalExpr, Ordinal),
    require_aggregate_ordinal(Ordinal).
head_arg_value(agg(group_concat(Sep), ValueExpr), contrib(Value)) :- !,
    require_aggregate_separator(Sep),
    eval_expr(ValueExpr, Value).
head_arg_value(agg(group_concat_ordered(Sep), ValueExpr-OrdinalExpr),
               contrib(Ordinal-Value)) :- !,
    require_aggregate_separator(Sep),
    eval_expr(ValueExpr, Value), eval_expr(OrdinalExpr, Ordinal),
    require_aggregate_ordinal(Ordinal).
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
agg_compute(sum, Contributions, Sum) :-
    require_aggregate_numbers(sum, Contributions),
    sum_list(Contributions, Sum).
agg_compute(avg, Contributions, Average) :-
    require_aggregate_numbers(avg, Contributions),
    sum_list(Contributions, Sum),
    length(Contributions, Count),
    Average is Sum / Count.
agg_compute(min, Contributions, Min) :-
    require_aggregate_numbers(min, Contributions),
    min_list(Contributions, Min).
agg_compute(max, Contributions, Max) :-
    require_aggregate_numbers(max, Contributions),
    max_list(Contributions, Max).
agg_compute(json_array, Contributions, Array) :- msort(Contributions, Array).
agg_compute(json_group_array, Contributions, Array) :-
    maplist(json_canon, Contributions, Canonical),
    msort(Canonical, Array).
agg_compute(json_group_array_ordered, Contributions, Array) :-
    keysort(Contributions, Sorted),
    pairs_values(Sorted, Values),
    maplist(json_canon, Values, Array).
agg_compute(group_concat(Sep), Contributions, Text) :-
    msort(Contributions, Sorted),
    aggregate_text_join(Sep, Sorted, Text).
agg_compute(group_concat_ordered(Sep), Contributions, Text) :-
    keysort(Contributions, Sorted),
    pairs_values(Sorted, Values),
    aggregate_text_join(Sep, Values, Text).
agg_compute(json_object, Pairs, obj(Object)) :-
    sort(Pairs, Distinct), keysort(Distinct, Object),
    pairs_keys(Object, Keys),
    ( sort(Keys, DistinctKeys), length(Keys, N), length(DistinctKeys, N)
    -> true ; throw(json_object_dup_key(Keys)) ).

% The residue 0_program_check.pl's aggregate_operand_not_number cannot reach:
% an UNDECLARED column carrying text into sum/avg/min/max. Without it
% sum_list/2 and min_list/3 evaluate the term and the door answers
% error(type_error(evaluable, alpha/0), context(lists:min_list/3, _)), which
% says nothing about the program. Named here in the same shape as the two
% value guards below, which are the group_concat and ordinal twins of it.
require_aggregate_numbers(Kind, Values) :-
    forall(member(Value, Values),
           ( number(Value)
           -> true
           ;  throw(aggregate_value_not_number(Kind, Value))
           )).

require_aggregate_separator(Sep) :-
    ( nonvar(Sep), atomic(Sep), \+ number(Sep)
    -> true
    ;  throw(aggregate_separator_not_constant(Sep))
    ).

require_aggregate_ordinal(Ordinal) :-
    ( integer(Ordinal)
    -> true
    ;  throw(aggregate_ordinal_not_int(Ordinal))
    ).

aggregate_text_join(Sep, Values, Text) :-
    maplist(aggregate_text_value, Values, Texts),
    atomic_list_concat(Texts, Sep, Text).

aggregate_text_value(Value, Text) :-
    ( atomic(Value)
    -> format(atom(Text), '~w', [Value])
    ;  throw(aggregate_value_not_text(Value))
    ).
