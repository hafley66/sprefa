:- module(dl7_integer_comparison,
          [ integer_comparison/1,
            integer_comparison_holds/3,
            integer_comparison_operator/3
          ]).

%% integer_comparison(?Name) is nondet.
%%
%% Registry order is kernel order. Each row carries the positive scalar
%% operator, its logical complement for grounded negation, and the compare/3
%% orderings accepted by the positive predicate.
integer_comparison_spec(int_lt, lt, ge, [<]).
integer_comparison_spec(int_le, le, gt, [<, =]).
integer_comparison_spec(int_eq, eq, ne, [=]).
integer_comparison_spec(int_ne, ne, eq, [<, >]).
integer_comparison_spec(int_ge, ge, lt, [=, >]).
integer_comparison_spec(int_gt, gt, le, [>]).

integer_comparison(Name) :-
    integer_comparison_spec(Name, _, _, _).

%% integer_comparison_holds(+Name, +Left, +Right) is semidet.
integer_comparison_holds(Name, Left, Right) :-
    integer(Left),
    integer(Right),
    integer_comparison_spec(Name, _, _, Orders),
    compare(Order, Left, Right),
    memberchk(Order, Orders).

%% integer_comparison_operator(+Name, +Polarity, -Operator) is semidet.
integer_comparison_operator(Name, positive, Operator) :-
    integer_comparison_spec(Name, Operator, _, _).
integer_comparison_operator(Name, negative, Operator) :-
    integer_comparison_spec(Name, _, Operator, _).
