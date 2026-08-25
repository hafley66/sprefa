% 0_seq_expand.pl : shared edge-body ordinal expansion.
%
%   numbered(Ordinal, Payload) <+ arrival(Payload), Ordinal := seq('q').
%
% expands to the four ordinary rules used by both the reference engine and the
% compiler. The cursor is a visible keyed relation because its name and rows
% are part of the tick-log contract.

:- module(seq_expand,
          [ expand_seq_in_context/3 ]).

:- use_module(library(lists)).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

expand_seq_in_context(_, prog(Decls0, Rules0), prog(Decls, Rules)) :-
    original_refs(Decls0, Rules0, OriginalRefs),
    expand_rules(Rules0, Decls0, OriginalRefs, Decls, Rules).

expand_rules([], Decls, _, Decls, []).
expand_rules([Rule | Rest], Decls0, OriginalRefs, Decls, Rules) :-
    ( seq_rule_parts(Rule, HeadName, HeadArgs, OrdinalPosition,
                     _OrdinalVariable, Partition, Body)
    -> cursor_ref(HeadName, OrdinalPosition, CursorName),
       CursorRef = CursorName/2,
       refuse_author_collision(CursorRef, OriginalRefs),
       infer_partition_type(Partition, Decls0, Body, PartitionType),
       add_cursor_decls(CursorRef, PartitionType, Decls0, Decls1),
       four_rules(HeadName, HeadArgs, OrdinalPosition, CursorName,
                  Partition, Body, ExpandedRules),
       expand_rules(Rest, Decls1, OriginalRefs, Decls2, RestRules),
       append(ExpandedRules, RestRules, Rules),
       Decls = Decls2
    ;  seq_in_level_rule(Rule)
    -> throw(unsupported_construct(seq_in_level_rule))
    ;  expand_rules(Rest, Decls0, OriginalRefs, Decls, RestRules),
       Rules = [Rule | RestRules]
    ).

% The generated cursor is shared by every seq rule targeting the same head
% relation and ordinal column. A declaration already present in the surface
% program is an author-owned name and therefore a collision.
add_cursor_decls(CursorRef, _PartitionType, Decls, Decls) :-
    memberchk(keyed(CursorRef, _), Decls),
    !.
add_cursor_decls(CursorRef, PartitionType, Decls, ExpandedDecls) :-
    append(Decls,
           [ col_type(CursorRef, partition, PartitionType),
             col_type(CursorRef, at, int),
             keyed(CursorRef, [1]) ],
           ExpandedDecls).

four_rules(HeadName, HeadArgs, OrdinalPosition, CursorName, Partition,
           Body, [CursorRule, CursorStepRule, BaseHeadRule, StepHeadRule]) :-
    CursorHead =.. [CursorName, Partition, 1],
    CursorEmpty =.. [CursorName, Partition, _CursorAt],
    CursorRule = (CursorHead <+ (Body, not(CursorEmpty))),
    CursorStepHead =.. [CursorName, Partition, CursorAt],
    CursorStepRule =
        (CursorStepHead <+ (Body, pre(CursorPrevious),
                            CursorAt := CursorPreviousValue + 1)),

    CursorPrevious =.. [CursorName, Partition, CursorPreviousValue],
    CursorPreviousHead =.. [CursorName, Partition, CursorPreviousHeadValue],

    FirstOrdinal = 1,
    replace_nth1(OrdinalPosition, HeadArgs, FirstOrdinal, BaseArgs),
    BaseHead =.. [HeadName | BaseArgs],
    BaseEmpty =.. [CursorName, Partition, _HeadAt],
    BaseHeadRule = (BaseHead <+ (Body, not(BaseEmpty))),

    replace_nth1(OrdinalPosition, HeadArgs, NextOrdinal, StepArgs),
    StepHead =.. [HeadName | StepArgs],
    StepHeadRule =
        (StepHead <+ (Body, pre(CursorPreviousHead),
                      NextOrdinal := CursorPreviousHeadValue + 1)).

% Match exactly one `Ordinal := seq(Partition)` bind and require the same
% variable in the edge head. The nonvar checks prevent an unrelated bind from
% being unified into seq(_) while inspecting the rule.
seq_rule_parts((Head <+ Body0), HeadName, HeadArgs, OrdinalPosition,
               OrdinalVariable, Partition, Body) :-
    compound(Head),
    Head =.. [HeadName | HeadArgs],
    conjunction_goals(Body0, Goals),
    select(Bind, Goals, Rest),
    nonvar(Bind),
    Bind = (OrdinalVariable := SeqCall),
    nonvar(SeqCall),
    SeqCall = seq(Partition),
    nth1(OrdinalPosition, HeadArgs, HeadOrdinal),
    HeadOrdinal == OrdinalVariable,
    goals_conjunction(Rest, Body),
    !.

seq_in_level_rule((Head <- Body)) :-
    conjunction_goals(Body, Goals),
    member(Bind, Goals),
    nonvar(Bind),
    Bind = (_ := SeqCall),
    nonvar(SeqCall),
    SeqCall = seq(_),
    !,
    ignore(Head = Head).

conjunction_goals(Body, Goals) :-
    ( nonvar(Body), Body = (Left, Right)
    -> conjunction_goals(Left, LeftGoals),
       conjunction_goals(Right, RightGoals),
       append(LeftGoals, RightGoals, Goals)
    ; Body == true
    -> Goals = []
    ; Goals = [Body]
    ).

goals_conjunction([], true) :- !.
goals_conjunction([Goal], Goal) :- !.
goals_conjunction([Goal | Rest], (Goal, Conjunction)) :-
    goals_conjunction(Rest, Conjunction).

replace_nth1(1, [_ | Rest], Value, [Value | Rest]) :- !.
replace_nth1(Position, [Head | Rest], Value, [Head | Replaced]) :-
    Position > 1,
    Next is Position - 1,
    replace_nth1(Next, Rest, Value, Replaced).

cursor_ref(HeadName, OrdinalPosition, CursorName) :-
    atomic_list_concat(['seq_', HeadName, '_', OrdinalPosition], CursorName).

refuse_author_collision(CursorRef, OriginalRefs) :-
    ( memberchk(CursorRef, OriginalRefs)
    -> throw(unsupported_construct(seq_cursor_name_collision(CursorRef)))
    ; true
    ).

original_refs(Decls, Rules, Refs) :-
    findall(Ref, (member(Decl, Decls), declaration_ref(Decl, Ref)), DeclRefs),
    findall(Ref, (member(Rule, Rules), term_ref(Rule, Ref)), RuleRefs),
    append(DeclRefs, RuleRefs, AllRefs),
    sort(AllRefs, Refs).

declaration_ref(kind(Ref, _), Ref).
declaration_ref(keep(Ref, _), Ref).
declaration_ref(keyed(Ref, _), Ref).
declaration_ref(col_type(Ref, _, _), Ref).

term_ref(Term, Ref) :-
    compound(Term),
    functor(Term, Name, Arity),
    atom(Name),
    Ref = Name/Arity.
term_ref(Term, Ref) :-
    compound(Term),
    Term =.. [_ | Args],
    member(Arg, Args),
    term_ref(Arg, Ref).

infer_partition_type(Value, _Decls, _Body, text) :-
    atom(Value),
    !.
infer_partition_type(Value, _Decls, _Body, text) :-
    string(Value),
    !.
infer_partition_type(Value, _Decls, _Body, int) :-
    integer(Value),
    !.
infer_partition_type(Value, _Decls, _Body, float) :-
    float(Value),
    !.
infer_partition_type(Value, _Decls, _Body, bool) :-
    nonvar(Value),
    Value = bool_lit(_),
    !.
infer_partition_type(Value, Decls, Body, Type) :-
    var(Value),
    conjunction_goals(Body, Goals),
    member(Goal, Goals),
    compound(Goal),
    Goal =.. [Name | Args],
    nth1(Position, Args, Argument),
    Argument == Value,
    length(Args, Arity),
    declared_columns(Decls, Name/Arity, Columns),
    nth1(Position, Columns, Column),
    memberchk(col_type(Name/Arity, Column, Type), Decls),
    !.
infer_partition_type(Value, _Decls, _Body, _) :-
    throw(unsupported_construct(seq_partition_type_unknown(Value))).

declared_columns(Decls, Ref, Columns) :-
    findall(Column, member(col_type(Ref, Column, _), Decls), Columns),
    Columns \== [].
