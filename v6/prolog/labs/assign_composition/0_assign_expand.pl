% 0_assign_expand.pl : the prototype `:=` desugaring, written to PRICE the
% migration rather than to ship it. Follows the 0_enum_expand / 0_match_expand
% precedent (one shared expansion module, consumed by both doors) so that if
% the user rules `:=` out, this file moves to v6/prolog/ unchanged in shape.
%
% THE WHOLE EXPANSION: a goal `Variable := Expression` (or the `is/2` alias)
% whose left side is an unbound variable is erased, and the variable is bound
% to the expression term itself. Prolog's own variable sharing performs the
% substitution at every remaining occurrence in the clause -- head argument,
% comparison operand, or a later `:=` right-hand side -- because those
% occurrences ARE the same variable cell.
%
% WHY THAT IS SOUND HERE, stated rather than assumed: lower.pl's compile_expr/4
% is the ONE expression compiler for head arguments, `:=` right-hand sides and
% comparison operands alike (lower.pl:360 header), and the oracle's
% body.pl:eval_expr/2 is its clause-for-clause mirror, reached from eval_head/2
% for head arguments and from solve/2 for `:=`. Substituting the expression
% into those positions therefore reaches the same compiler with the same term.
%
% THE ONE POSITION WHERE IT IS NOT SOUND, and so a named refusal: a body ATOM
% argument is a PATTERN, not an expression. `over_ten(Base + 1)` does not
% compute; it destructures a stored compound (measured: the compiler reaches
% join_column_type_mismatch on json_extract(..., '$.args[0]'), never
% arithmetic). Substituting into that position would silently change a
% computation into a match, so it is refused by name.

:- module(assign_expand,
          [ expand_assign_program/2,
            expand_assign_rule/2 ]).

:- use_module(library(lists)).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% Each rule is COPIED before expansion. Variables are rule-scoped in this
% language (the text door's parse_dl gives every rule a fresh binding
% environment), but a TERM-door program is one prolog term, so two rules
% written with the same variable names share the same cells. Measured on
% counter_fold_matches_hand_computation: expanding rule 1 in place bound the
% shared `Next` and left rule 2 reading `Total+1 := Total-1`, which silently
% lost the whole decrement arm. enum and match expansion never hit this
% because neither of them BINDS a variable.
expand_assign_program(prog(Decls, Rules0), prog(Decls, Rules)) :-
    maplist(expand_assign_rule_copied, Rules0, Rules).

expand_assign_rule_copied(Rule0, Rule) :-
    copy_term(Rule0, RuleCopy),
    expand_assign_rule(RuleCopy, Rule).

expand_assign_rule((Head <- Body0), (Head <- Body)) :-
    !,
    expand_assign_body(Head, Body0, Body).
expand_assign_rule((Head <+ Body0), (Head <+ Body)) :-
    !,
    expand_assign_body(Head, Body0, Body).
expand_assign_rule(Rule, Rule).

% Goals are folded LEFT TO RIGHT, which is the order the oracle solves them
% (body.pl header: "goals run left to right"). A later `:=` may read a name an
% earlier one bound, so the earlier binding must already be in place; folding
% right to left would leave the chained case unexpanded.
expand_assign_body(Head, Body0, Body) :-
    body_goal_list(Body0, Goals0),
    foldl(expand_assign_goal(Head, Goals0), Goals0, [], Kept0),
    reverse(Kept0, Kept),
    ( Kept == []
    -> throw(unsupported_construct(assign_expansion_emptied_body(Head)))
    ;  body_conjunction(Kept, Body)
    ).

expand_assign_goal(Head, AllGoals, Goal, Acc, Acc) :-
    assign_goal(Goal, Variable, Expression),
    var(Variable),
    !,
    refuse_if_pattern_position(Head, AllGoals, Goal, Variable),
    Variable = Expression.
expand_assign_goal(_, _, Goal, Acc, [Goal | Acc]).

assign_goal(Variable := Expression, Variable, Expression).
assign_goal(Variable is Expression, Variable, Expression).

% A body atom argument is a pattern position. If the bound variable occurs in
% one, refuse rather than substitute. The check runs BEFORE the binding, while
% the variable is still a distinguishable cell.
refuse_if_pattern_position(Head, AllGoals, Goal, Variable) :-
    (  member(Other, AllGoals),
       Other \== Goal,
       body_atom_goal(Other),
       term_contains_var(Other, Variable)
    -> throw(unsupported_construct(
                assign_var_in_pattern_position(Head, Goal, Other)))
    ;  true
    ).

% A body ATOM is anything that is not one of the expression-position goal
% families. Guards and binds take expressions; everything else (a plain rel
% atom, pre/1, latest/1, not/1, finalize/1, match arms) matches patterns.
body_atom_goal(Goal) :-
    nonvar(Goal),
    \+ assign_goal(Goal, _, _),
    \+ comparison_goal(Goal).

comparison_goal(Goal) :-
    nonvar(Goal),
    functor(Goal, Operator, 2),
    memberchk(Operator, ['<', '=<', '>', '>=', '==', '\\==']).

term_contains_var(Term, Variable) :-
    term_variables(Term, Variables),
    member(Occurrence, Variables),
    Occurrence == Variable,
    !.

body_goal_list(Body, Goals) :-
    ( nonvar(Body), Body = (First, Rest)
    -> body_goal_list(Rest, RestGoals),
       Goals = [First | RestGoals]
    ;  Goals = [Body]
    ).

body_conjunction([Goal], Goal) :- !.
body_conjunction([Goal | Rest], (Goal, RestBody)) :-
    body_conjunction(Rest, RestBody).
