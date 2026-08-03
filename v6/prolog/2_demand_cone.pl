% 2_demand_cone.pl : the demand cone, compute only, consumed by nothing yet.
%
% Seeded by the program's queries, closed over the rule graph: a demanded
% rel demands every rule whose head is that rel, and each such rule demands
% every rel its body reads, through samplers and negation alike. The compat
% rule keeps today's semantics: a program with no query demands everything.
:- module('2_demand_cone', [ demand_cone/4, op(1150, xfx, <-) ]).

:- op(1150, xfx, <-).

%! demand_cone(+Decls, +Rules, +Queries, -DemandedRels) is det.
%  DemandedRels is a sorted list of Name/Arity.
demand_cone(Decls, _Rules, [], Cone) :-
    !,
    declared_rels(Decls, Rels),
    sort(Rels, Cone).
demand_cone(Decls, Rules, Queries, Cone) :-
    declared_rels(Decls, RelList),
    sort(RelList, Declared),
    findall(Name/Arity,
            ( member(QueryAtom, Queries), functor(QueryAtom, Name, Arity) ),
            SeedList),
    sort(SeedList, Seeds),
    cone_fixpoint(Seeds, Rules, Declared, Cone).

% Declared = every rel a kind/2 decl names; the cone membership test runs
% against it so a guard or arithmetic goal never counts as a rel read.
declared_rels(Decls, Rels) :-
    findall(Ref, member(kind(Ref, _Kind), Decls), Rels).

cone_fixpoint(Cone0, Rules, Declared, Cone) :-
    findall(BodyRef,
            ( member((Head <- Body), Rules),
              functor(Head, HeadName, HeadArity),
              memberchk(HeadName/HeadArity, Cone0),
              body_rel(Body, Declared, BodyRef) ),
            Reached),
    append(Cone0, Reached, Widened),
    sort(Widened, Cone1),
    ( Cone1 == Cone0
    -> Cone = Cone0
    ;  cone_fixpoint(Cone1, Rules, Declared, Cone)
    ).

% Structural walk of a rule body; every solution is one rel the body reads.
body_rel((Left, Right), Declared, Ref) :-
    !,
    ( body_rel(Left, Declared, Ref) ; body_rel(Right, Declared, Ref) ).
body_rel((Left ; Right), Declared, Ref) :-
    !,
    ( body_rel(Left, Declared, Ref) ; body_rel(Right, Declared, Ref) ).
body_rel(not(Goal), Declared, Ref) :-
    !,
    body_rel(Goal, Declared, Ref).
body_rel(\+(Goal), Declared, Ref) :-
    !,
    body_rel(Goal, Declared, Ref).
body_rel(pre(Goal), Declared, Ref) :-
    !,
    body_rel(Goal, Declared, Ref).
body_rel(latest(Goal), Declared, Ref) :-
    !,
    body_rel(Goal, Declared, Ref).
body_rel(finalize(Goal), Declared, Ref) :-
    !,
    body_rel(Goal, Declared, Ref).
body_rel(Atom, Declared, Name/Arity) :-
    compound(Atom),
    functor(Atom, Name, Arity),
    memberchk(Name/Arity, Declared).
