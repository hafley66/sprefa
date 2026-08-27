% Reference stratum groups plus the topological rule order SQL emission runs.
% A positive cycle inside a stratum has no such order: cyclic_head_groups/2.

:- module(strat, [ stratum_groups/2, sql_rule_order/2, recursive_stratum_groups/2,
                   cyclic_head_groups/2 ]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(library(pairs)).
:- use_module('../3_analyze/analyze', [ rule_is_level/1, rule_head_ref/2, body_ref_uses/2,
                         rule_is_aggregate/1 ]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% ═══ stratum_groups/2 : mirrors level_eval.pl's relax_strata exactly ════════

stratum_groups(Rules, Groups) :-
    include(rule_is_level, Rules, LevelRules),
    findall(Ref, ( member(Rule, LevelRules), rule_head_ref(Rule, Ref) ), DerivedRefs0),
    sort(DerivedRefs0, DerivedRefs),
    findall(HeadRef-constraint(BodyRef, Gap),
            ( member(Rule, LevelRules), Rule = (_Head <- Body),
              rule_head_ref(Rule, HeadRef),
              body_ref_uses(Body, Uses),
              member(use(BodyRef, _, Sign, _), Uses),
              rule_gap_of(Rule, Sign, Gap),
              memberchk(BodyRef, DerivedRefs)
            ), Constraints0),
    sort(Constraints0, Constraints),
    length(DerivedRefs, DerivedCount),
    Cap is DerivedCount + 1,
    findall(Ref-0, member(Ref, DerivedRefs), Strata0),
    relax_strata(Constraints, Cap, Strata0, StrataMap),
    findall(Number-LevelRule,
            ( member(LevelRule, LevelRules), rule_head_ref(LevelRule, Ref),
              memberchk(Ref-Number, StrataMap) ),
            Numbered),
    keysort(Numbered, Sorted),
    group_pairs_by_key(Sorted, Grouped),
    pairs_values(Grouped, Groups).

% level_eval.pl:rule_body_constraint/4 forces Gap=1 for EVERY body ref of an
% AGGREGATE head, positive or negated ("negated and aggregated rels strictly
% below their consumers"), because an aggregate must see its input complete
% before it folds. Mirroring that exactly is what keeps this file's stratum
% numbers identical to the oracle's; treating an aggregate's positive read as
% Gap=0 would let the aggregate and its input land in one group and be
% emitted in a single pass, folding a half-built rel.
rule_gap_of(Rule, _Sign, 1) :- rule_is_aggregate(Rule), !.
rule_gap_of(_Rule, Sign, Gap) :- gap_of(Sign, Gap).

gap_of(pos, 0).
gap_of(neg, 1).

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

% ═══ sql_rule_order/2 : topological sub-order within each stratum group ════

sql_rule_order(Rules, Ordered) :-
    stratum_groups(Rules, Groups),
    maplist(topo_order_group, Groups, OrderedGroups),
    append(OrderedGroups, Ordered).

topo_order_group(Group, Ordered) :-
    findall(Ref, ( member(Rule, Group), rule_head_ref(Rule, Ref) ), HeadRefsInOrder),
    sort(HeadRefsInOrder, HeadRefs),
    group_head_edges(Group, HeadRefs, Edges),
    kahn_order(HeadRefs, Edges, PlacedRefs),
    % Kahn stops early on a cycle; ordering by HEAD keeps every rule of one
    % head adjacent, which lower.pl:group_adjacent_by_head/2 reads.
    append_unplaced(HeadRefsInOrder, PlacedRefs, RefOrder),
    findall(Rule,
            ( member(Ref, RefOrder), member(Rule, Group), rule_head_ref(Rule, Ref) ),
            Ordered).

group_head_edges(Group, HeadRefs, Edges) :-
    findall(HeadRef-DependsOnRef,
            ( member(Rule, Group), Rule = (_ <- Body), rule_head_ref(Rule, HeadRef),
              body_ref_uses(Body, Uses), member(use(DependsOnRef, _, pos, _), Uses),
              memberchk(DependsOnRef, HeadRefs), DependsOnRef \== HeadRef ),
            Edges0),
    sort(Edges0, Edges).

append_unplaced(HeadRefsInOrder, PlacedRefs, RefOrder) :-
    findall(Ref, ( member(Ref, HeadRefsInOrder), \+ memberchk(Ref, PlacedRefs) ),
            Unplaced0),
    dedupe_keep_order(Unplaced0, Unplaced),
    append(PlacedRefs, Unplaced, RefOrder).

dedupe_keep_order([], []).
dedupe_keep_order([Ref | Rest], [Ref | Out]) :-
    exclude(==(Ref), Rest, Filtered),
    dedupe_keep_order(Filtered, Out).

recursive_stratum_groups(Rules, RecursiveGroups) :-
    stratum_groups(Rules, Groups),
    include(recursive_stratum_group, Groups, RecursiveGroups).

recursive_stratum_group(Group) :-
    findall(Ref, ( member(Rule, Group), rule_head_ref(Rule, Ref) ), HeadRefs0),
    sort(HeadRefs0, HeadRefs),
    group_head_edges(Group, HeadRefs, Edges),
    kahn_order(HeadRefs, Edges, RefOrder),
    length(RefOrder, OrderedCount),
    length(HeadRefs, HeadCount),
    OrderedCount < HeadCount.

% Head refs on a positive INDIRECT cycle, paired with their stratum group index.
% Direct self-recursion is absent: group_head_edges/3 drops the self edge.
cyclic_head_groups(Rules, HeadGroups) :-
    stratum_groups(Rules, Groups),
    findall(Ref-GroupIndex,
            ( nth0(GroupIndex, Groups, Group),
              recursive_stratum_group(Group),
              member(Rule, Group),
              rule_head_ref(Rule, Ref) ),
            Pairs),
    sort(Pairs, HeadGroups).

% Kahn's algorithm: emit a ref once every ref it positively depends on
% (within this group) has already been emitted.
kahn_order(Refs, Edges, Order) :- kahn_order(Refs, Edges, [], Order).

kahn_order([], _, Acc, Order) :- reverse(Acc, Order).
kahn_order(Remaining, Edges, Acc, Order) :-
    Remaining \== [],
    ( select(Ref, Remaining, Rest),
      \+ ( member(Ref-DependsOn, Edges), memberchk(DependsOn, Remaining) )
    -> kahn_order(Rest, Edges, [Ref | Acc], Order)
    ; reverse(Acc, Order) ).
