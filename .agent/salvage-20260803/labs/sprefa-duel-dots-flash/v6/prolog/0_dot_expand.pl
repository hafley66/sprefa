% 0_dot_expand.pl : dotted member access `X.field` / `X.a.b.c` desugars to
% the decode/2 brace spelling, so the lowering pipeline sees nothing new.
%
% The parser already built the glued chain as a nested proj/2 term (the third
% spelling of the same read). This phase rewrites every proj/2 chain in a rule
% head and body into a fresh leaf variable plus a decode/2 goal whose brace
% pattern nests one level per field:
%
%   head:  dcoord(F.at.name, S, E) <- span(F, S, E).
%   body:  dcoord(N, S, E) <- span(F, S, E), decode(F, {at: {name: N}}).
%
% which is term-for-term the same program the brace/decode fixture produces, so
% the equivalence gate is structural rather than a new golden.
%
% Resolution is bound-variable-first per the ruling M3d: the chain's ROOT must
% be a variable bound by the rule body, else the named refusal
% unresolvable_member. There is no module half in scope, so a chain whose root
% is not a bound body variable is never silently repairable.
%
% A rule that carries no proj term is returned byte-identical, so no existing
% fixture's body shape moves.

:- module(dot_expand,
          [ expand_dot_in_context/3 ]).

:- use_module(library(lists)).
:- use_module(library(occurs), [sub_term/2]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

expand_dot_in_context(_, prog(Decls, Rules0), prog(Decls, Rules)) :-
    maplist(expand_dot_rule, Rules0, Rules).

expand_dot_rule(Rule0, Rule) :-
    ( has_proj_term(Rule0)
    -> desugar_dot_rule(Rule0, Rule)
    ; Rule = Rule0
    ).

% The project-wide guard: a rule only descends into the desugar when it
% actually carries a proj/2 chain. A bare `proj(_, _)` pattern unifies with an
% unbound variable (a variable matches any term), which would route EVERY rule
% through the rewrite and corrupt decode-bearing bodies, so the match insists
% the subterm be a non-variable compound of functor proj/2.
has_proj_term(Term) :-
    sub_term(Sub, Term),
    nonvar(Sub),
    functor(Sub, proj, 2),
    !.

desugar_dot_rule((Head0 <- Body0), (Head <- Body)) :- !,
    desugar_body(Head0, Body0, Head, Body).
desugar_dot_rule((Head0 <+ Body0), (Head <+ Body)) :- !,
    desugar_body(Head0, Body0, Head, Body).
desugar_dot_rule(Rule, Rule).

desugar_body(Head0, Body0, Head, Body) :-
    conjunction_goals(Body0, Goals0),
    term_variables(Body0, Bound),
    rewrite_head(Head0, Bound, Head, HeadGoals),
    rewrite_goal_list(Bound, Goals0, GoalsRewritten, BodyGoals),
    append(GoalsRewritten, HeadGoals, MidGoals),
    append(MidGoals, BodyGoals, FinalGoals),
    goals_conjunction(FinalGoals, Body).

rewrite_head(Head0, Bound, Head, Goals) :-
    Head0 =.. [Name | Args],
    maplist(rewrite_term(Bound), Args, OutArgs, GoalLists),
    append(GoalLists, Goals),
    Head =.. [Name | OutArgs].

rewrite_goal_list(_, [], [], []).
rewrite_goal_list(Bound, [Goal | Rest], [OutGoal | OutRest], AllGoals) :-
    rewrite_term(Bound, Goal, OutGoal, Goals),
    rewrite_goal_list(Bound, Rest, OutRest, RestGoals),
    append(Goals, RestGoals, AllGoals).

% ŌöĆ─ the conjunction spine ─────────────────────────────────────────────────
% Flattened to a list and rebuilt only on rules that actually carry a proj
% chain; a rule without one returns byte-identical (the maplist guard above).

conjunction_goals(Body, Goals) :-
    ( nonvar(Body), Body = (Left, Right)
    -> conjunction_goals(Left, LeftGoals),
       conjunction_goals(Right, RightGoals),
       append(LeftGoals, RightGoals, Goals)
    ;  Body == true
    -> Goals = []
    ;  Goals = [Body]
    ).

goals_conjunction([], true) :- !.
goals_conjunction([Goal], Goal) :- !.
goals_conjunction([Goal | Rest], (Goal, Conjunction)) :-
    goals_conjunction(Rest, Conjunction).

% ── the rewrite ──────────────────────────────────────────────────────────────
% rewrite_term walks an arbitrary term, replacing every proj/2 chain with a
% fresh leaf variable and collecting one decode/2 goal per chain.

rewrite_term(_Bound, Term, Term, []) :- ( var(Term) ; atomic(Term) ), !.
rewrite_term(Bound, Term, Out, Goals) :-
    ( Term = proj(_, _)
    ->  chain_parts(Term, Root, Fields),
        resolve_root(Root, Fields, Bound),
        leaf_var(Leaf),
        nested_brace(Fields, Leaf, Brace),
        Out = Leaf,
        Goals = [decode(Root, Brace)]
    ;   compound(Term),
        Term =.. [Functor | Args],
        maplist(rewrite_term(Bound), Args, OutArgs, GoalLists),
        append(GoalLists, Goals),
        Out =.. [Functor | OutArgs]
    ).

% chain_parts decomposes proj(proj(A, b), c) into Root=A and Fields=[b, c].
chain_parts(Term, Root, Fields) :-
    ( nonvar(Term), Term = proj(Receiver, Field)
    -> chain_parts(Receiver, Root, Prefix),
       append(Prefix, [Field], Fields)
    ; Root = Term, Fields = []
    ).

% Bound-variable-first: the root must be a variable the body already binds.
resolve_root(Root, Fields, Bound) :-
    ( var(Root), memberchk_eq(Root, Bound)
    -> true
    ; member_name(Root, Fields, Name),
      throw(unsupported_construct(unresolvable_member(Name)))
    ).

member_name(Root, Fields, Name) :-
    ( atom(Root)
    -> atomic_list_concat([Root | Fields], '.', Name)
    ; atomic_list_concat(Fields, '.', Name)
    ).

memberchk_eq(Variable, [Head | Rest]) :-
    ( Head == Variable -> true ; memberchk_eq(Variable, Rest) ).

leaf_var(Leaf) :-
    copy_term(_, Leaf).

nested_brace([Field], Leaf, Brace) :-
    Brace = '{}'((Field : Leaf)).
nested_brace([Field | Rest], Leaf, Brace) :-
    nested_brace(Rest, Leaf, Inner),
    Brace = '{}'((Field : Inner)).
