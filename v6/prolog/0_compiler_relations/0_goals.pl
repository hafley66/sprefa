% Compiler-plane goal classification, authored-order safety, and shared scalar
% evaluation. Relation goals bind their variables; scalar binds and guards may
% only read variables bound by prior goals.

compiler_body_goals(true, []) :- !.
compiler_body_goals((Left, Right), Goals) :- !,
    compiler_body_goals(Left, LeftGoals),
    compiler_body_goals(Right, RightGoals),
    append(LeftGoals, RightGoals, Goals).
compiler_body_goals(Goal, [Goal]).

validate_compiler_goal_sequence([], Bound, Bound).
validate_compiler_goal_sequence([Goal | Rest], Bound0, Bound) :-
    validate_compiler_goal(Goal, Bound0, Bound1),
    validate_compiler_goal_sequence(Rest, Bound1, Bound).

validate_compiler_goal(type_apply(Constructor, Arguments, Application),
                       Bound0, Bound) :-
    !,
    term_variables(application(Constructor, Arguments), InputVariables),
    ( is_list(Arguments), variables_are_bound(InputVariables, Bound0)
    -> add_term_variables(Application, Bound0, Bound)
    ; throw(unsupported_construct(type_apply_non_ground_application(
                type_apply(Constructor, Arguments, Application))))
    ).
validate_compiler_goal(Goal, Bound0, Bound) :-
    compiler_bind_goal(Goal, Variable, Expression),
    !,
    term_variables(Expression, InputVariables),
    ( variables_are_bound(InputVariables, Bound0)
    -> add_term_variables(Variable, Bound0, Bound)
    ; throw(unsupported_construct(compiler_expression_non_ground(Expression)))
    ).
validate_compiler_goal(Goal, Bound, Bound) :-
    comparison_goal(Goal),
    !,
    term_variables(Goal, Variables),
    ( variables_are_bound(Variables, Bound)
    -> true
    ; throw(unsupported_construct(compiler_comparison_non_ground(Goal)))
    ).
validate_compiler_goal(Goal, Bound0, Bound) :-
    add_term_variables(Goal, Bound0, Bound).

variables_are_bound([], _).
variables_are_bound([Variable | Rest], Bound) :-
    member_variable(Variable, Bound),
    variables_are_bound(Rest, Bound).

add_term_variables(Term, Bound0, Bound) :-
    term_variables(Term, Variables),
    add_variables(Variables, Bound0, Bound).

add_variables([], Bound, Bound).
add_variables([Variable | Rest], Bound0, Bound) :-
    ( member_variable(Variable, Bound0)
    -> Bound1 = Bound0
    ; Bound1 = [Variable | Bound0]
    ),
    add_variables(Rest, Bound1, Bound).

member_variable(Variable, [Candidate | _]) :- Variable == Candidate, !.
member_variable(Variable, [_ | Rest]) :- member_variable(Variable, Rest).

body_atoms(true, []) :- !.
body_atoms((Left, Right), Atoms) :- !,
    body_atoms(Left, LeftAtoms),
    body_atoms(Right, RightAtoms),
    append(LeftAtoms, RightAtoms, Atoms).
body_atoms(Goal, []) :- compiler_bind_goal(Goal, _, _), !.
body_atoms(Goal, []) :- comparison_goal(Goal), !.
body_atoms(Atom, [Atom]).

compiler_bind_goal(Goal, Variable, Expression) :-
    body_surface_for_term(Goal, _, bind, no_refs, infix(_), _),
    arg(1, Goal, Variable),
    arg(2, Goal, Expression).

eval_ground_expression(Expression, Value) :-
    ( ground(Expression)
    -> eval_expr(Expression, Value)
    ; throw(unsupported_construct(compiler_expression_non_ground(Expression)))
    ).

holds_ground_comparison(Goal) :-
    ( ground(Goal)
    -> solve_comparison(Goal)
    ; throw(unsupported_construct(compiler_comparison_non_ground(Goal)))
    ).
