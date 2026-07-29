% guard_check.pl : sabotage receipt for the ONE refusal 0_assign_expand.pl
% carries. Zero fixtures in the corpus put a `:=`-bound variable into a body
% ATOM argument, so the guard would be vacuous without a deliberate case.
% Run: swipl -q -l guard_check.pl -g check -g halt

:- use_module('0_assign_expand', [expand_assign_rule/2]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

check :-
    Rule = (flagged(Name) <- seen(Name, Base, _), Sum := Base + 1, over_ten(Sum)),
    (   catch(( expand_assign_rule(Rule, Expanded),
                format("GUARD IS VACUOUS -- expanded to ~q~n", [Expanded]) ),
              Error,
              format("GUARD FIRES: ~q~n", [Error]))
    ->  true
    ;   format("expansion failed outright~n")
    ),
    Safe = (flagged(Name2, Sum2) <- seen(Name2, Base2, _), Sum2 := Base2 + 1),
    catch(( expand_assign_rule(Safe, SafeExpanded),
            format("CONTROL expands: ~q~n", [SafeExpanded]) ),
          SafeError,
          format("CONTROL WRONGLY REFUSED: ~q~n", [SafeError])).
