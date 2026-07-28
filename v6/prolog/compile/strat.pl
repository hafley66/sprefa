% strat.pl : level-rule execution order for SQL emission.
%
% Two orderings are computed and they answer different questions:
%
%   stratum_groups/2  reproduces level_eval.pl:stratify_level_rules/2's OWN
%   grouping exactly (same relax_strata gap algorithm, reimplemented here
%   since that predicate is not exported): S(head) >= S(body rel) for a
%   positive read, strictly greater under not/1. This is the number the
%   reference engine assigns; report it verbatim (RETURN section: "stratum
%   orders found").
%
%   sql_rule_order/2 answers a NARROWER question the reference engine never
%   has to ask, because it re-evaluates a whole stratum group to a joint
%   fixpoint (plain_fixpoint) rather than emitting SQL statements in a fixed
%   sequence. A generated program executes each rule's SQL exactly ONCE per
%   tick, so within a stratum group the rules need a topological
%   sub-order over POSITIVE-only edges (a negated read already point to a
%   strictly lower, already-settled stratum, so it never needs sequencing
%   within the group). For the two target fixtures this sub-order coincides
%   with dependency order (demanded before route_view / demanded before
%   effect_call) because both fixtures are acyclic; a genuine positive cycle
%   inside one group (real mutual recursion, which relax_strata is willing to
%   leave at the same number since Gap=0 never forces separation) is NOT
%   something this compiler lowers -- sql_rule_order/2 throws
%   unsupported_construct(recursive_stratum(Refs)) rather than silently
%   emitting a wrong single-pass order.

:- module(strat, [ stratum_groups/2, sql_rule_order/2 ]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(library(pairs)).
:- use_module(analyze, [ rule_is_level/1, rule_head_ref/2, body_ref_uses/2,
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
    findall(Ref, ( member(Rule, Group), rule_head_ref(Rule, Ref) ), HeadRefs0),
    sort(HeadRefs0, HeadRefs),
    findall(HeadRef-DependsOnRef,
            ( member(Rule, Group), Rule = (_ <- Body), rule_head_ref(Rule, HeadRef),
              body_ref_uses(Body, Uses), member(use(DependsOnRef, _, pos, _), Uses),
              memberchk(DependsOnRef, HeadRefs), DependsOnRef \== HeadRef ),
            Edges0),
    sort(Edges0, Edges),
    kahn_order(HeadRefs, Edges, RefOrder),
    ( length(RefOrder, N), length(HeadRefs, N)
    -> true
    ; throw(unsupported_construct(recursive_stratum(HeadRefs)))
    ),
    findall(Rule,
            ( member(Ref, RefOrder), member(Rule, Group), rule_head_ref(Rule, Ref) ),
            Ordered).

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
