% 2_subscribe.pl : the subscribe cone, compute only, consumed by nothing yet.
%
% Seeded by the program's queries, closed over the rule graph: a subscribed
% rel subscribes to every rule whose head is that rel, and each such rule
% subscribes to every rel its body reads, through samplers and negation alike,
% plus the one edge no rule body carries: a subscribed host RESPONSE rel
% subscribes to its host's DEMAND rel (host_edge/3 below).
% Strict per ruling zero_query_semantics 2026-08-03: a program with no query
% subscribes to NOTHING; harness-side subscription roots arrive with the
% pruning rung.
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
:- use_module('0_dot_expand/0_body_walk', [body_relation_atoms/4]).
:- use_module('2_host_expand/1_host_expand', [host_relation_refs/3]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).

%! subscribed_rels(+Decls, +Rules, +Queries, -SubscribedRels) is det.
%  SubscribedRels is a sorted list of Name/Arity.
%  Strict per ruling zero_query_semantics 2026-08-03: no query, nothing.
subscribed_rels(_Decls, _Rules, [], Cone) :-
    !,
    Cone = [].
subscribed_rels(Decls, Rules, Queries, Cone) :-
    findall(Name/Arity,
            ( member(QueryAtom, Queries), functor(QueryAtom, Name, Arity) ),
            SeedList),
    sort(SeedList, Seeds),
    cone_fixpoint(Seeds, Decls, Rules, Cone).

% Both arrows: `<+` edge rules carry as much of a real program's subscribe
% chain as `<-` level rules do (golden-flex.dl6 reaches pick_count, last_picker
% and every pre/1 read exclusively through them).
cone_rule((Head <- Body), Name/Arity, Body) :- functor(Head, Name, Arity).
cone_rule((Head <+ Body), Name/Arity, Body) :- functor(Head, Name, Arity).

cone_fixpoint(Cone0, Decls, Rules, Cone) :-
    findall(BodyRef,
            ( member(Rule, Rules),
              cone_rule(Rule, HeadRef, Body),
              memberchk(HeadRef, Cone0),
              body_rel(Body, BodyRef) ),
            Reached),
    findall(DemandRef,
            ( member(Ref, Cone0), host_edge(Decls, Ref, DemandRef) ),
            HostReached),
    append([Cone0, Reached, HostReached], Widened),
    sort(Widened, Cone1),
    ( Cone1 == Cone0
    -> Cone = Cone0
    ;  cone_fixpoint(Cone1, Decls, Rules, Cone)
    ).

% THE EDGE NO RULE BODY CARRIES. 1_host_expand.pl splits a probe into a demand
% rule and a response atom, and the response relation is EDB: nothing in the
% rule graph reads the demand relation, so the walk above stops at the response
% and the demand rel falls outside the cone. Under a replay schedule the answers
% arrive anyway and the gap is invisible; against a live host the demand rows
% are what the runner reads, so a pruned demand rel means the host never runs
% and the subscribed rel stays empty with no error.
%
% The pairing is read from the host DECLARATION, so it holds for every host in
% every program at the rel-kind level, and the two names come from
% host_relation_refs/3 rather than a second spelling of the prefixes.
% Demand arity is read from the generated col_type/3 decls because a probe's
% salts widen the relation past its base arity.
host_edge(Decls, ResponseName/_, DemandName/DemandArity) :-
    member(sh_decl(HostName, _, _, _), Decls),
    host_relation_refs(HostName, DemandName, ResponseName),
    member(col_type(DemandName/DemandArity, _, _), Decls).

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
