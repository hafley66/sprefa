% 0_dot_expand.pl : dotted member access `Record.field` / `Record.a.b.c`, the
% dot spelling of the decode/2 brace pattern.
%
% dot_get(Receiver, Field) is the parsed form of one hop (parse_dl.pl
% dot_chain/4 nests one per field; print_dl.pl prints the nest back). This
% phase erases every dot_get before checks, typing, or either lowering door can
% see one:
%
%   head arg   dcoord(FileRec.at.name, S, E) <- span(FileRec, S, E).
%   becomes    dcoord(Leaf, S, E) <- span(FileRec, S, E),
%                                    decode(FileRec, {at: {name: Leaf}}).
%
%   whole-RHS bind   PathName := FileRec.at.name
%   becomes          decode(FileRec, {at: {name: PathName}})
%
%   any other position   a fresh leaf variable plus that same decode goal
%                        beside the host goal
%
% so the desugared program is term-identical to the brace spelling and nothing
% past expansion learns a new construct.
%
% Placement of a synthesized decode: AFTER a plain relation atom (that atom may
% be the goal that binds the receiver, as in `f(X, X.a)`), BEFORE any other
% goal (a bind, guard, or negation reads its operands, so the leaf has to be
% bound first). A decode synthesized from a HEAD argument appends after the
% whole body: the leaf is read by the head alone.
%
% Resolution is receiver-bound-first: the chain's ROOT must be a variable the
% rule body binds, else the named unsupported construct unresolvable_member. There is no
% overlap with a FUNCTOR-position path: that one has an atom root by
% construction and resolves against the decl tree instead.
%
% A rule that carries no dot_get is returned byte-identical, so no existing
% fixture's body shape moves.
%
% ── what is refused ──────────────────────────────────────────────────────────
%
%   unresolvable_member   the root is not a variable this body binds. An ATOM
%                         root spells the whole path in the payload; a variable
%                         root spells the fields alone, since the parse keeps
%                         variable IDENTITY and not surface names, and a
%                         payload has to be ground for a fixture to pin it
%                         (engine.pl grades throws/1 by ==/2).
%   member_not_a_goal     a dot chain sitting where a goal belongs has no value
%                         position to desugar into. Text-door programs cannot
%                         reach it (a dot chain at goal position is a parse
%                         error); a term-door fixture can write one.
%   unresolvable_path     a functor-position path walks off the decl tree. The
%                         payload keeps every segment.

:- module(dot_expand,
          [ expand_dot_in_context/3 ]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(library(occurs), [sub_term/2]).
:- use_module('compile/registry', [surface/5]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

expand_dot_in_context(EnumContext, prog(Decls, Rules0), prog(Decls, Rules)) :-
    maplist(resolve_enum_arm_term(EnumContext), Rules0, Rules1),
    (   member(Carrier, Rules1),
        contains_rel_path(Carrier)
    ->  decl_scope_tree(Decls, Root),
        maplist(resolve_rel_path_rule([Root]), Rules1, Rules2)
    ;   Rules2 = Rules1
    ),
    maplist(expand_dot_rule, Rules2, Rules).

% `Enum.variant(...)` is the arm's own spelling. The generated ref comes from
% enum_context, so this cannot drift from what expansion actually minted.
resolve_enum_arm_term(EnumContext, Term0, Term) :-
    (   nonvar(Term0),
        Term0 = rel_path(Segments, Args),
        enum_arm_ref(EnumContext, Segments, Args, Resolved)
    ->  Term = Resolved
    ;   compound(Term0)
    ->  Term0 =.. [Functor | Args0],
        maplist(resolve_enum_arm_term(EnumContext), Args0, Args1),
        Term =.. [Functor | Args1]
    ;   Term = Term0
    ).

enum_arm_ref(EnumContext, [EnumName, VariantName], Args, Resolved) :-
    memberchk(EnumName-VariantRefs, EnumContext),
    memberchk(VariantRelName/VariantArity-VariantName, VariantRefs),
    length(Args, VariantArity),
    Resolved =.. [VariantRelName | Args].

% Either door: the text door parses rel_path/2, and SWI reads `a.b(X)` as
% '.'(a, b(X)), which would otherwise become a rel literally named '.'.
resolve_rel_path_rule(Scopes, Rule0, Rule) :-
    (   contains_rel_path(Rule0)
    ->  rewrite_rel_paths(Scopes, Rule0, Rule)
    ;   Rule = Rule0
    ).

contains_rel_path(Term) :-
    sub_term(Sub, Term),
    nonvar(Sub),
    rel_path_parts(Sub, _, _),
    !.

rewrite_rel_paths(Scopes, Term0, Term) :-
    (   nonvar(Term0),
        rel_path_parts(Term0, Segments, Args)
    ->  (   resolve_path(Scopes, Segments, Name)
        ->  maplist(rewrite_rel_paths(Scopes), Args, ResolvedArgs),
            Term =.. [Name | ResolvedArgs]
        ;   throw(unsupported_construct(unresolvable_path(Segments)))
        )
    ;   compound(Term0)
    ->  Term0 =.. [Functor | Args0],
        maplist(rewrite_rel_paths(Scopes), Args0, Args1),
        Term =.. [Functor | Args1]
    ;   Term = Term0
    ).

rel_path_parts(rel_path(Segments, Args), Segments, Args) :-
    is_list(Segments),
    is_list(Args).
% A literal '.'(A, B) in a clause head would itself be dict-expanded by SWI,
% which is the trap this predicate exists to catch, so the shape is inspected.
rel_path_parts(Term, [Receiver | Rest], Args) :-
    compound(Term),
    functor(Term, '.', 2),
    arg(1, Term, Receiver),
    atom(Receiver),
    arg(2, Term, Applied),
    nonvar(Applied),
    (   rel_path_parts(Applied, Rest, Args)
    ->  true
    ;   compound(Applied),
        Applied =.. [LocalName | Args],
        Args \== [],
        Rest = [LocalName]
    ).

% Scopes run innermost first, so a nearer room's name binds before an outer
% same-name; one file with no block surface = the file room alone.
decl_scope_tree(Decls, Root) :-
    findall(Segments-Name, declared_path(Decls, Segments, Name), Paths0),
    sort(Paths0, Paths),
    foldl(insert_path, Paths, node(file, none, []), Root).

declared_path(Decls, Segments, Name) :-
    member(rel_path_decl(Name/_, Segments), Decls).
declared_path(Decls, [Name], Name) :-
    declared_flat_name(Decls, Name),
    \+ member(rel_path_decl(Name/_, _), Decls).

declared_flat_name(Decls, Name) :- member(col_type(Name/_, _, _), Decls).
declared_flat_name(Decls, Name) :- member(kind(Name/_, _), Decls).
declared_flat_name(Decls, Name) :- member(keyed(Name/_, _), Decls).
declared_flat_name(Decls, Name) :- member(keep(Name/_, _), Decls).
declared_flat_name(Decls, Name) :- member(type_decl(Name, _), Decls).
declared_flat_name(Decls, Name) :- member(enum_decl(Name, _), Decls).

insert_path(Segments-Name, node(Local, Resolved, Children0),
            node(Local, Resolved, Children)) :-
    insert_segments(Segments, Name, Children0, Children).

insert_segments([Segment], Name, Children0, Children) :-
    !,
    (   selectchk(node(Segment, _, Grand), Children0, Others)
    ->  Children = [node(Segment, some(Name), Grand) | Others]
    ;   Children = [node(Segment, some(Name), []) | Children0]
    ).
insert_segments([Segment | Rest], Name, Children0, Children) :-
    (   selectchk(node(Segment, Resolved, Grand0), Children0, Others)
    ->  insert_segments(Rest, Name, Grand0, Grand),
        Children = [node(Segment, Resolved, Grand) | Others]
    ;   insert_segments(Rest, Name, [], Grand),
        Children = [node(Segment, none, Grand) | Children0]
    ).

resolve_path([Scope | Outer], Segments, Name) :-
    (   descend(Scope, Segments, Name)
    ->  true
    ;   resolve_path(Outer, Segments, Name)
    ).

descend(node(_, _, Children), [Segment | Rest], Name) :-
    memberchk(node(Segment, Resolved, Grand), Children),
    (   Rest == []
    ->  Resolved = some(Name)
    ;   descend(node(Segment, Resolved, Grand), Rest, Name)
    ).

expand_dot_rule(Rule0, Rule) :-
    ( contains_dot_get(Rule0)
    -> desugar_dot_rule(Rule0, Rule)
    ;  Rule = Rule0
    ).

desugar_dot_rule((Head0 <- Body0), (Head <- Body)) :- !,
    desugar_head_and_body(Head0, Body0, Head, Body).
desugar_dot_rule((Head0 <+ Body0), (Head <+ Body)) :- !,
    desugar_head_and_body(Head0, Body0, Head, Body).
desugar_dot_rule(Rule, Rule).

desugar_head_and_body(Head0, Body0, Head, Body) :-
    conjunction_goals(Body0, Goals0),
    bound_body_vars(Goals0, BoundVars),
    maplist(rewrite_goal(BoundVars), Goals0, GoalLists),
    append(GoalLists, BodyGoals),
    rewrite_head(Head0, BoundVars, Head, HeadDecodes),
    append(BodyGoals, HeadDecodes, FinalGoals),
    goals_conjunction(FinalGoals, Body).

% ── head arguments ───────────────────────────────────────────────────────────
% A head dot chain is ruled IN: `dcoord(FileRec.at.name, S, E) <- span(...)`.
% The receiver still has to be bound by the BODY, which is why the head is
% rewritten against the same BoundVars set the body goals used.

rewrite_head(Head0, BoundVars, Head, Decodes) :-
    ( compound(Head0)
    -> Head0 =.. [Name | Args0],
       foldl(replace_dot_gets_arg(BoundVars), Args0, Args, [], Decodes),
       Head =.. [Name | Args]
    ;  Head = Head0, Decodes = []
    ).

% ── goal rewriting ───────────────────────────────────────────────────────────

rewrite_goal(_, Goal, [Goal]) :-
    \+ contains_dot_get(Goal),
    !.
rewrite_goal(_, Goal, _) :-
    nonvar(Goal),
    Goal = dot_get(_, _),
    !,
    dot_path_atom(Goal, Path),
    throw(unsupported_construct(member_not_a_goal(Path))).
% The whole-RHS bind is rewritten IN PLACE with the bind's own left side as the
% pattern leaf, which is what makes the dot twin of a decode fixture expand to
% the brace original term for term. Any other bind shape (a dot inside a larger
% expression) falls to the generic clause, where the decode lands BEFORE the
% bind that reads its leaf.
rewrite_goal(BoundVars, (Lhs := Rhs), [decode(Root, Pattern)]) :-
    nonvar(Rhs),
    Rhs = dot_get(_, _),
    \+ contains_dot_get(Lhs),
    !,
    dot_chain_parts(Rhs, Root, Fields),
    check_dot_receiver(BoundVars, Root, Rhs),
    fields_pattern(Fields, Lhs, Pattern).
rewrite_goal(BoundVars, Goal0, GoalList) :-
    replace_dot_gets(Goal0, BoundVars, Goal, Decodes),
    ( plain_relation_goal(Goal)
    -> GoalList = [Goal | Decodes]
    ;  append(Decodes, [Goal], GoalList)
    ).

replace_dot_gets(Term, _, Term, []) :-
    var(Term),
    !.
replace_dot_gets(Term, BoundVars, Leaf, [decode(Root, Pattern)]) :-
    nonvar(Term),
    Term = dot_get(_, _),
    !,
    dot_chain_parts(Term, Root, Fields),
    check_dot_receiver(BoundVars, Root, Term),
    fields_pattern(Fields, Leaf, Pattern).
replace_dot_gets(Term, BoundVars, Out, Decodes) :-
    compound(Term),
    !,
    Term =.. [Functor | Args0],
    foldl(replace_dot_gets_arg(BoundVars), Args0, Args, [], Decodes),
    Out =.. [Functor | Args].
replace_dot_gets(Term, _, Term, []).

replace_dot_gets_arg(BoundVars, Arg0, Arg, Acc, Decodes) :-
    replace_dot_gets(Arg0, BoundVars, Arg, ArgDecodes),
    append(Acc, ArgDecodes, Decodes).

% dot_chain_parts decomposes dot_get(dot_get(A, b), c) into Root=A, [b, c].
dot_chain_parts(Term, Root, Fields) :-
    ( nonvar(Term), Term = dot_get(Receiver, Field)
    -> dot_chain_parts(Receiver, Root, Prefix),
       append(Prefix, [Field], Fields)
    ;  Root = Term, Fields = []
    ).

fields_pattern([Field], Leaf, '{}'(Field:Leaf)) :-
    atom(Field),
    !.
fields_pattern([Field | Rest], Leaf, '{}'(Field:Sub)) :-
    atom(Field),
    Rest = [_ | _],
    !,
    fields_pattern(Rest, Leaf, Sub).
fields_pattern(Fields, _, _) :-
    fields_path_atom(Fields, Path),
    throw(unsupported_construct(unresolvable_member(Path))).

check_dot_receiver(BoundVars, Root, Chain) :-
    ( var(Root),
      memberchk_eq(Root, BoundVars)
    -> true
    ;  dot_path_atom(Chain, Path),
       throw(unsupported_construct(unresolvable_member(Path)))
    ).

% Payload convention: an ATOM root spells the whole path (`foo.bar`), a
% variable root spells the fields alone. The parse keeps variable IDENTITY, not
% surface names, so a variable-rooted path has no name to report and the
% payload has to stay ground for a fixture to pin it by ==/2.
dot_path_atom(Chain, Path) :-
    dot_chain_parts(Chain, Root, Fields),
    ( atom(Root)
    -> fields_path_atom([Root | Fields], Path)
    ;  fields_path_atom(Fields, Path)
    ).

fields_path_atom(Fields, Path) :-
    ( is_list(Fields),
      Fields \== [],
      maplist(atom, Fields)
    -> atomic_list_concat(Fields, '.', Path)
    ;  Path = '?'
    ).

% ── what binds a variable, for the receiver-bound-first check ────────────────
% Vars come from the dot-stripped goals: in `f(X.a)` the receiver X is READ
% through the dot, never bound by f. A bind's left side and a decode pattern
% both count, each binding its captures exactly as a synthesized decode will.

bound_body_vars(Goals, BoundVars) :-
    foldl(goal_bound_vars, Goals, [], BoundVars).

goal_bound_vars(Goal, Acc, BoundVars) :-
    binding_positions(Goal, Positions),
    strip_dot_gets(Positions, Stripped),
    term_variables(Stripped, GoalVars),
    append(Acc, GoalVars, BoundVars).

binding_positions(Goal, []) :- var(Goal), !.
binding_positions((Lhs := _), [Lhs]) :- !.
binding_positions(is(Lhs, _), [Lhs]) :- !.
binding_positions(not(_), []) :- !.
binding_positions(decode(_, Pattern), [Pattern]) :- !.
binding_positions(latest(Atom), [Atom]) :- !.
binding_positions(pre(Atom), [Atom]) :- !.
binding_positions(finalize(Atom), [Atom]) :- !.
binding_positions(next(Atom), [Atom]) :- !.
binding_positions(coalesce(Atom, _), [Atom]) :- !.
binding_positions(now(Value), [Value]) :- !.
binding_positions(probe(_, _, Outputs, _), [Outputs]) :- !.
binding_positions(Goal, Positions) :-
    functor(Goal, Functor, Arity),
    ( Functor == combine
    -> Goal =.. [_ | Positions]
    ;  surface(Functor/Arity, _, _, _, _)
    -> Positions = []
    ;  Goal =.. [_ | Positions]
    ).

strip_dot_gets(Term, Stripped) :-
    ( var(Term)
    -> Stripped = Term
    ;  Term = dot_get(_, _)
    -> Stripped = _Fresh
    ;  compound(Term)
    -> Term =.. [Functor | Args],
       maplist(strip_dot_gets, Args, StrippedArgs),
       Stripped =.. [Functor | StrippedArgs]
    ;  Stripped = Term
    ).

plain_relation_goal(Goal) :-
    nonvar(Goal),
    compound(Goal),
    functor(Goal, Functor, Arity),
    atom(Functor),
    \+ surface(Functor/Arity, _, _, _, _),
    Functor/Arity \== probe/4.

% ── the conjunction spine (same shape as 0_coalesce_expand.pl) ───────────────

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

% ── residuals ────────────────────────────────────────────────────────────────

% A bare dot_get/2 pattern unifies with an unbound variable, which would route
% every rule through the rewrite; the scan insists on a nonvar compound.
contains_dot_get(Term) :-
    sub_term(Sub, Term),
    nonvar(Sub),
    Sub = dot_get(_, _),
    !.

memberchk_eq(Variable, [Head | Rest]) :-
    ( Head == Variable -> true ; memberchk_eq(Variable, Rest) ).
