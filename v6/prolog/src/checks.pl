% checks.pl : reusable program-level checks for the kernel-4 surface.
% These are the analyses the labs proved (enum_match.pl exhaustiveness,
% initialization.pl's first-instant rule, the v5 twin post-mortems), packaged
% for import instead of restatement.

% body_member/2 is exported for examples/ghcacher.pl, which reads rule bodies
% to report which boundary relations are clocked on deltas. It reached it as a
% private qualified goal before rank R8 of
% plans/2026-07-29-prolog-org-review.md.
:- module(checks,
          [ covers_enum/2, has_subscribe_arm/1,
            no_twin_names/1, no_self_union/1,
            surface_grounded/0, body_member/2 ]).

:- use_module(library(lists)).
:- use_module(kernel).

:- op(1150, xfx, <-).

% every enum constructor has a next-arm (enum_match.pl's exhaustiveness)
covers_enum(Arms, Ctors) :-
    forall(member(Ctor, Ctors),
           ( functor(Ctor, Name, _),
             member(arm(next(Pattern), _, _), Arms),
             functor(Pattern, Name, _) )).

% a subscribe arm exists: defined at the first instant (initialization.pl's d)
has_subscribe_arm(Arms) :- memberchk(arm(subscribe, _), Arms).

% no *_next twin rels (the v5 carry-twin pattern, obsoleted by registers)
no_twin_names(Heads) :-
    \+ ( member(Head, Heads), functor(Head, Name, _),
         sub_atom(Name, _, _, 0, '_next') ).

% no rule unions a rel into itself (the v5 change_log self-carry)
no_self_union(Rules) :-
    \+ ( member(Head <- Body, Rules),
         functor(Head, Name, Arity),
         body_member(Ref, Body),
         functor(Ref, Name, Arity) ).

body_member(Ref, (A, B)) :- !, ( body_member(Ref, A) ; body_member(Ref, B) ).
body_member(Ref, Ref).

% every surface declaration form maps onto the 4-element kernel
surface_grounded :-
    forall(surface_form(_, Parts),
           forall(member(Part, Parts), kernel(Part))).
