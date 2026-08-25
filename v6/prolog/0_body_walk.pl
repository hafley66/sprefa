% Registry-driven traversal of a rule body shared by the reference engine and
% compiler.
%
% The contract:
%
%   walk_body(+Body, +WalkPolicy, -Events)
%
%   Events is a LEFT-TO-RIGHT list of
%
%       event(Path, Polarity, Surface, Term)
%
%     Path      list of argument indices from the body root, [] at the root,
%               innermost index last. Diagnostics can name a position with it.
%     Polarity  pos, or neg once the walk has descended through a not/1.
%               Absorbing, never flipping: both consumers that care read a
%               doubly negated atom as negative, matching what
%               analyze:body_ref_uses/2 and level_eval:goal_rel_refs/3 did
%               before this file existed.
%     Surface   surface(Signature, Axis, AnalyzeRole, LowerRole, Status) when
%               registry.pl knows the functor, or the atom plain_atom when it
%               does not. plain_atom is what makes a bare relation atom a
%               relation atom: unsupported construct by absence stays the registry's job.
%     Term      the term at that node, unchanged.
%
%   WalkPolicy is walk_policy(DescendNot, SpliceBare):
%
%     DescendNot   descend_not(true)  walk not/1's argument, marking neg
%                  descend_not(false) emit the not/1 node and stop there
%     SpliceBare   splice_bare(true)  walk the arguments of the splice roles,
%                                     which are next/1 and variadic combine
%                  splice_bare(false) emit the wrapper node and stop there
%
% A wrapper node is ALWAYS emitted, whether or not the walk descends through
% it. Consumers that only want leaves filter on Surface. That is what lets one
% traversal serve both the projections that read the wrapper (latest/1's
% argument is the sampled reference) and the projections that read past it.
%
% Conjunction is the one form the walk always flattens, and it is flattened by
% shape rather than by registry row: ','/2 carries no surface/5 row because it
% is not a construct, it is the body's spine. Left-nested and right-nested
% conjunctions therefore produce the same event list, which the
% comma_left/comma_right goldens pin.
%
:- module(body_walk,
          [ walk_body/3,
            body_conjunction_goals/3,
            body_wrapper_refs/4,
            relation_atom_wrapper/1,
            event_relation_atom/2,
            body_relation_atoms/4,
            body_reserved_word/4
          ]).

:- use_module(library(lists)).
:- use_module('0_dot_expand/registry', [body_surface_for_term/6]).

% ── the walk ─────────────────────────────────────────────────────────────────

walk_body(Body, Policy, Events) :-
    walk_node(Body, [], pos, Policy, Events, []).

% Difference list, so left-to-right order costs no appends.
walk_node(Term, Path, Polarity, Policy, Events, Tail) :-
    nonvar(Term),
    Term = (Left, Right),
    !,
    walk_node(Left,  [1 | Path], Polarity, Policy, Events, Middle),
    walk_node(Right, [2 | Path], Polarity, Policy, Middle, Tail).
walk_node(Term, Path, Polarity, Policy, [Event | Rest], Tail) :-
    node_surface(Term, Surface),
    reverse(Path, ForwardPath),
    Event = event(ForwardPath, Polarity, Surface, Term),
    walk_children(Surface, Term, Path, Polarity, Policy, Rest, Tail).

node_surface(Term, surface(Signature, Axis, AnalyzeRole, LowerRole, Status)) :-
    body_surface_for_term(Term, Signature, Axis, AnalyzeRole, LowerRole,
                          Status),
    !.
node_surface(_, plain_atom).

% not/1 descends under descend_not(true), absorbing polarity to neg.
walk_children(surface(_, _, arm(neg), _, _), Term, Path, _Polarity,
              Policy, Events, Tail) :-
    Policy = walk_policy(descend_not(true), _),
    !,
    arg(1, Term, Inner),
    walk_node(Inner, [1 | Path], neg, Policy, Events, Tail).
% next/1 and variadic combine splice their arguments in under
% splice_bare(true); polarity carries through unchanged.
walk_children(surface(_, _, splice_bare, _, _), Term, Path, Polarity,
              Policy, Events, Tail) :-
    Policy = walk_policy(_, splice_bare(true)),
    !,
    Term =.. [_ | Arguments],
    walk_arguments(Arguments, 1, Path, Polarity, Policy, Events, Tail).
walk_children(_, _, _, _, _, Events, Events).

walk_arguments([], _, _, _, _, Events, Events).
walk_arguments([Argument | Rest], Index, Path, Polarity, Policy,
               Events, Tail) :-
    walk_node(Argument, [Index | Path], Polarity, Policy, Events, Middle),
    Next is Index + 1,
    walk_arguments(Rest, Next, Path, Polarity, Policy, Middle, Tail).

% ── projections shared by more than one consumer ─────────────────────────────

% The ordered goal list of a body: one entry per node the walk reached that is
% not a conjunction. With splice_bare(true) a next/1 or combine contributes its
% ARGUMENTS and not itself, which is what analyze:conjunction_goals/2 means by
% splicing.
% Collected by recursion and NEVER by findall/3: findall copies its template,
% and every consumer of a goal list needs the goals to keep sharing the body's
% own variables.
body_conjunction_goals(Body, Policy, Goals) :-
    walk_body(Body, Policy, Events),
    events_goals(Events, Policy, Goals).

events_goals([], _, []).
events_goals([Event | Rest], Policy, Goals) :-
    (   spliced_goal(Event, Policy, Goal)
    ->  Goals = [Goal | RestGoals]
    ;   Goals = RestGoals
    ),
    events_goals(Rest, Policy, RestGoals).

% Under splice_bare(true) the wrapper itself is dropped and its arguments,
% already walked in, stand in for it. That is what splicing means.
spliced_goal(event(_, _, Surface, Term), walk_policy(_, splice_bare(true)),
             Term) :-
    !,
    Surface \= surface(_, _, splice_bare, _, _).
spliced_goal(event(_, _, _, Term), _, Term).

% ── the wrapper family that carries a relation atom ──────────────────────────
%
% ONE statement of which wrappers hold a relation ATOM as their single
% argument. This list was written out three times -- in the shared program
% check, in the compiler's relation-pattern residue check, and in the oracle's
% relation-value rewriter -- and burr B11 of
% plans/2026-07-30-relpattern-adversarial-review.md is exactly that: three
% traversals, three policies, and the asymmetries between them observable as
% behaviour.
%
% NOT DERIVED from registry.pl, and the reason is not laziness. The rows whose
% LowerRole is wrapper(rel_atom, _) also include next/1, whose argument the
% walk SPLICES in as its own event (counting the wrapper too would count that
% atom twice), and the four reserved lifecycle wrappers, which are refused
% before anything asks them for a relation atom. The set below is therefore
% stated, and compile/test/plunit_tests.pl's
% relation_atom_wrapper_family_matches_the_registry pins it against the
% registry rows so the two cannot drift apart in silence.
relation_atom_wrapper(latest).
relation_atom_wrapper(pre).
relation_atom_wrapper(finalize).

% The relation atom one event carries: the term itself when the walk found a
% bare atom, or the wrapper's argument for the family above. Fails for every
% other node, which is what makes it a filter as well as a projection.
event_relation_atom(event(_, _, Surface, Term), Atom) :-
    (   Surface == plain_atom
    ->  Atom = Term
    ;   nonvar(Term),
        functor(Term, Wrapper, 1),
        relation_atom_wrapper(Wrapper),
        arg(1, Term, Atom)
    ).

% Every relation atom a body reaches, with the POLARITY the walk gave it, in
% source order. Polarity is an argument rather than a filter so a caller can
% ask for the negated ones specifically (a relation value under not/1 is a
% named unsupported construct) without a second traversal.
body_relation_atoms(Body, Policy, Polarity, Atom) :-
    walk_body(Body, Policy, Events),
    member(Event, Events),
    Event = event(_, Polarity, _, _),
    event_relation_atom(Event, Atom).

% Every RESERVED registry word one body reaches, in source order, with the
% lowering role that says which unsupported construct family it belongs to. `reserved` is
% the status for a word the language has CLAIMED and given no meaning; both
% doors refuse on it, and neither may execute it.
%
% Shared because it had exactly the drift shape rank R1 exists to stop: the
% compiler walked for these words and the reference engine did not walk at
% all, so five claimed words compiled to a named unsupported construct and ran as silently
% empty relations. 0_program_check.pl's reserved_body_word trigger and
% analyze.pl's reserved_construct_in_body/2 are both this predicate now; the
% latter is what the body_walk_characterization golden pins, so the two
% cannot drift apart again without that test going red.
%
% The POLICY stays the caller's, and the two callers pass the same one. It is
% an argument rather than a constant because the narrowness (not/1 opaque,
% splices unwalked) is a stated boundary of those callers, not of this
% projection.
body_reserved_word(Body, Policy, Functor/Arity, LowerRole) :-
    walk_body(Body, Policy, Events),
    member(event(_, _, surface(Functor/Arity, _, _, LowerRole, reserved), _),
           Events).

% Every reference carried by one wrapper family, in source order: the argument
% of each latest/1, pre/1, pre/2 or finalize/1 the walk reached. This is the shared
% implementation behind four predicates that the review found written twice,
% once in the oracle and once in the compiler.
body_wrapper_refs(Body, Wrapper, Policy, Ref) :-
    walk_body(Body, Policy, Events),
    member(event(_, _, _, Term), Events),
    nonvar(Term),
    functor(Term, Wrapper, WrapperArity),
    wrapper_arity(Wrapper, WrapperArity),
    arg(1, Term, Atom),
    nonvar(Atom),
    functor(Atom, Name, Arity),
    Ref = Name/Arity.

wrapper_arity(pre, Arity) :- !, memberchk(Arity, [1, 2]).
wrapper_arity(_, 1).
