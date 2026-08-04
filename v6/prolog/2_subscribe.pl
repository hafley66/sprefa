% 2_subscribe.pl : the subscribe cone, compute only, consumed by nothing yet.
%
% Seeded by the program's queries, closed over the rule graph: a subscribed
% rel subscribes to every rule whose head is that rel, and each such rule
% subscribes to every rel its body reads, through samplers and negation alike.
% The compat rule keeps today's semantics: a program with no query subscribes
% to everything.
%
% SHARED with the reference engine (engine.pl:run_program/5 computes the same
% cone), so this module depends only on modules the oracle already loads --
% 0_body_walk.pl, whose header states that sharing. That is the
% 1_host_expand.pl precedent, and it is why the body walk here is
% 0_body_walk.pl's registry-driven one rather than a second hand-written
% traversal: analyze.pl:body_ref_uses/2 reads the same walk with the same
% policy, so compiler and oracle cannot disagree about what a body reads.
:- module('2_subscribe',
          [ subscribed_rels/4, op(1150, xfx, <-), op(1150, xfx, <+) ]).

:- use_module(library(lists)).
:- use_module('0_body_walk', [body_relation_atoms/4]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).

%! subscribed_rels(+Decls, +Rules, +Queries, -SubscribedRels) is det.
%  SubscribedRels is a sorted list of Name/Arity.
subscribed_rels(Decls, Rules, [], Cone) :-
    !,
    program_rels(Decls, Rules, Cone).
subscribed_rels(_Decls, Rules, Queries, Cone) :-
    findall(Name/Arity,
            ( member(QueryAtom, Queries), functor(QueryAtom, Name, Arity) ),
            SeedList),
    sort(SeedList, Seeds),
    cone_fixpoint(Seeds, Rules, Cone).

% The compat value: every rel the program declares OR mentions. Mirrors
% analyze.pl:declared_refs/2's four decl forms (a rel can be declared by any
% of them independently) unioned with the rule graph's own refs, which is
% compile.pl:program_plan/2's AllRefs minus the schedule-seeded refs -- a rel
% seeded only by a world row is not something a query could subscribe to.
% declared_rels_match_analyze pins the decl half against analyze.pl.
program_rels(Decls, Rules, Rels) :-
    findall(Ref, ( declared_rel(Decls, Ref) ; rule_rel(Rules, Ref) ), Refs),
    sort(Refs, Rels).

declared_rel(Decls, Ref) :-
    member(Decl, Decls),
    (   Decl = kind(Ref, _)
    ;   Decl = keyed(Ref, _)
    ;   Decl = keep(Ref, _)
    ;   Decl = col_type(Ref, _, _)
    ).

rule_rel(Rules, Ref) :-
    member(Rule, Rules),
    (   cone_rule(Rule, Ref, _)
    ;   cone_rule(Rule, _, Body), body_rel(Body, Ref)
    ).

% Both arrows: `<+` edge rules carry as much of a real program's subscribe
% chain as `<-` level rules do (golden-flex.dl6 reaches pick_count, last_picker
% and every pre/1 read exclusively through them).
cone_rule((Head <- Body), Name/Arity, Body) :- functor(Head, Name, Arity).
cone_rule((Head <+ Body), Name/Arity, Body) :- functor(Head, Name, Arity).

cone_fixpoint(Cone0, Rules, Cone) :-
    findall(BodyRef,
            ( member(Rule, Rules),
              cone_rule(Rule, HeadRef, Body),
              memberchk(HeadRef, Cone0),
              body_rel(Body, BodyRef) ),
            Reached),
    append(Cone0, Reached, Widened),
    sort(Widened, Cone1),
    ( Cone1 == Cone0
    -> Cone = Cone0
    ;  cone_fixpoint(Cone1, Rules, Cone)
    ).

% The widest walk policy, the one analyze.pl:body_ref_uses/2 uses: descend
% not/1, splice next/1 and combine. The registry decides what is a relation
% atom, so a guard, a `:=` bind, now/1 and decode/2's json pattern contribute
% nothing without this file naming any of them.
body_rel(Body, Name/Arity) :-
    body_relation_atoms(Body,
                        walk_policy(descend_not(true), splice_bare(true)),
                        _Polarity, Atom),
    compound(Atom),
    functor(Atom, Name, Arity).
