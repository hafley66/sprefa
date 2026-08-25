% Finite compiler-plane aggregation. Count heads read completed lower strata;
% their consumers enter the positive tabled closure only after those rows are
% fixed.

validate_compiler_aggregate_heads([]).
validate_compiler_aggregate_heads([Rule | Rest]) :-
    rule_head(Rule, Head),
    validate_compiler_aggregate_head(Head),
    validate_compiler_aggregate_heads(Rest).

validate_compiler_aggregate_head(Head) :-
    Head =.. [_ | Arguments],
    forall(( member(Argument, Arguments),
             surface_for_term(Argument, Signature, aggregate, _, _, _) ),
           ( Signature == count/1
           -> true
           ; throw(unsupported_construct(
                       compiler_aggregate_unsupported(Signature)))
           )).

%! evaluate_compiler_strata(+Rules, +Seeds, -Rows) is det.
%  A strict dependency edge enters each count-headed rule. The completed rows
%  below a stratum feed its counts once; ordinary rules in that stratum then
%  close under the existing tabled evaluator.
evaluate_compiler_strata(Rules, Seeds, Rows) :-
    compiler_rule_strata(Rules, Strata),
    evaluate_compiler_strata_groups(Strata, Seeds, Rows).

evaluate_compiler_strata_groups([], Rows0, Rows) :- sort(Rows0, Rows).
evaluate_compiler_strata_groups([RuleGroup | Rest], Rows0, Rows) :-
    include(compiler_aggregate_rule, RuleGroup, AggregateRules),
    exclude(compiler_aggregate_rule, RuleGroup, PlainRules),
    findall(Row,
            ( member(Rule, AggregateRules),
              derive_compiler_aggregate_row(Rows0, Rule, Row) ),
            AggregateRows),
    append(Rows0, AggregateRows, StratumSeeds0),
    sort(StratumSeeds0, StratumSeeds),
    tabled_compiler_closure(PlainRules, StratumSeeds, StratumRows),
    evaluate_compiler_strata_groups(Rest, StratumRows, Rows).

compiler_rule_strata([], []).
compiler_rule_strata(Rules, Strata) :-
    findall(Ref, ( member(Rule, Rules), rule_head_ref(Rule, Ref) ), Refs0),
    sort(Refs0, DerivedRefs),
    findall(HeadRef-constraint(BodyRef, Gap),
            ( member(Rule, Rules),
              rule_head_ref(Rule, HeadRef),
              rule_body(Rule, Body),
              compiler_rule_constraint(Rule, Body, BodyRef, Gap),
              memberchk(BodyRef, DerivedRefs) ),
            Constraints),
    length(DerivedRefs, DerivedCount),
    Cap is DerivedCount + 1,
    findall(Ref-0, member(Ref, DerivedRefs), Strata0),
    relax_compiler_strata(Constraints, Cap, Strata0, StrataMap),
    findall(Number-Rule,
            ( member(Rule, Rules),
              rule_head_ref(Rule, Ref),
              memberchk(Ref-Number, StrataMap) ),
            Numbered),
    keysort(Numbered, Sorted),
    group_pairs_by_key(Sorted, Grouped),
    pairs_values(Grouped, Strata).

compiler_rule_constraint(Rule, Body, BodyRef, Gap) :-
    body_atoms(Body, Atoms),
    member(Atom, Atoms),
    atom_ref(Atom, BodyRef),
    ( compiler_aggregate_rule(Rule) -> Gap = 1 ; Gap = 0 ).

relax_compiler_strata(Constraints, Cap, Strata0, Strata) :-
    findall(changed,
            ( member(HeadRef-constraint(BodyRef, Gap), Constraints),
              memberchk(HeadRef-HeadStratum, Strata0),
              memberchk(BodyRef-BodyStratum, Strata0),
              HeadStratum < BodyStratum + Gap ),
            Changes),
    ( Changes == []
    -> Strata = Strata0
    ; findall(Ref-Number,
              ( member(Ref-Current, Strata0),
                findall(Needed,
                        ( member(Ref-constraint(BodyRef, Gap), Constraints),
                          memberchk(BodyRef-BodyStratum, Strata0),
                          Needed is BodyStratum + Gap ),
                        Neededs),
                max_list([Current | Neededs], Number) ),
              Strata1),
      ( member(_-Number, Strata1), Number > Cap
      -> throw(unsupported_construct(compiler_aggregate_not_stratified))
      ; relax_compiler_strata(Constraints, Cap, Strata1, Strata)
      )
    ).

compiler_aggregate_rule(Rule) :-
    rule_head(Rule, Head),
    compiler_aggregate_head(Head, _).

compiler_aggregate_head(Head, Template) :-
    compound(Head),
    Head =.. [_ | Arguments],
    maplist(compiler_head_argument, Arguments, Template),
    memberchk(agg(count, _), Template).

compiler_head_argument(Argument, agg(count, Expression)) :-
    nonvar(Argument),
    surface_for_term(Argument, count/1, aggregate, no_refs, head(_), _),
    !,
    arg(1, Argument, Expression).
compiler_head_argument(Argument, plain(Argument)).

derive_compiler_aggregate_row(Rows, Rule0, Row) :-
    copy_term(Rule0, Rule),
    rule_head(Rule, Head),
    rule_body(Rule, Body),
    compiler_aggregate_head(Head, Template),
    Head =.. [Name | _],
    findall(Contribution,
            ( satisfy_compiler_body(Rows, Body),
              maplist(compiler_head_argument_value, Template, Contribution) ),
            Bag),
    Bag \== [],
    findall(GroupKey-Solution,
            ( member(Solution, Bag),
              compiler_aggregate_group_key(Template, Solution, GroupKey) ),
            Keyed),
    keysort(Keyed, Sorted),
    group_pairs_by_key(Sorted, Groups),
    member(_-Group, Groups),
    compiler_aggregate_arguments(Template, Group, Arguments),
    Row =.. [Name | Arguments].

compiler_head_argument_value(plain(Expression), value(Value)) :-
    eval_ground_expression(Expression, Value).
compiler_head_argument_value(agg(count, Expression), contribution(Value)) :-
    eval_ground_expression(Expression, Value).

compiler_aggregate_group_key(Template, Solution, GroupKey) :-
    findall(Value,
            ( nth1(Position, Template, plain(_)),
              nth1(Position, Solution, value(Value)) ),
            GroupKey).

compiler_aggregate_arguments(Template, Group, Arguments) :-
    findall(Argument,
            ( nth1(Position, Template, TemplateArgument),
              compiler_aggregate_argument(TemplateArgument, Position, Group,
                                          Argument) ),
            Arguments).

compiler_aggregate_argument(plain(_), Position, [Solution | _], Value) :-
    nth1(Position, Solution, value(Value)).
compiler_aggregate_argument(agg(count, _), Position, Group, Count) :-
    findall(Contribution,
            ( member(Solution, Group),
              nth1(Position, Solution, contribution(Contribution)) ),
            Contributions),
    length(Contributions, Count).
