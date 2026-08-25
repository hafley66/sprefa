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
          [ expand_dot_in_context/3,
            % 1_expansion.pl runs this ahead of every phase: generic expansion
            % mints an artifact NAME from a wrapper's element type.
            resolve_qualified_types/2,
            % use_resolve.pl reads a mounted subtree's paths off the same
            % projection the scope tree is built from.
            declared_path/3,
            % Query declarations are compiled before the ordinary expansion
            % fold, so host preparation resolves their path carriers through
            % the same tree as rule atoms.
            resolve_relation_paths/3 ]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(library(occurs), [sub_term/2]).
:- use_module('compile/registry', [surface/5]).
:- use_module('0_type_plane',
              [ column_element_type_name/2,
                type_definitions/2, type_definition/4, declared_type_name/2,
                relation_columns_and_types/5 ]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

expand_dot_in_context(EnumContext, prog(Decls0, Rules0), prog(Decls, Rules)) :-
    resolve_qualified_type_paths(Decls0, Decls1),
    maplist(resolve_enum_arm_term(EnumContext), Rules0, Rules1),
    (   member(Carrier, Rules1),
        contains_rel_path(Carrier)
    ->  decl_scope_tree(Decls1, Root),
        maplist(resolve_rel_path_rule([Root]), Rules1, Rules2)
    ;   Rules2 = Rules1
    ),
    expand_nested_parent_refs(Decls1, Rules2, Decls, Rules3),
    maplist(expand_dot_rule(Decls), Rules3, Rules).

resolve_qualified_types(prog(Decls0, Rules), prog(Decls, Rules)) :-
    resolve_qualified_type_paths(Decls0, Decls).
resolve_qualified_types(program(Decls0, Rules, Queries),
                        program(Decls, Rules, Queries)) :-
    resolve_qualified_type_paths(Decls0, Decls).

% Qualified types retain their path until mount_decl/4 supplies scope here.
% Resolution produces the same flat relation identity as a qualified call.
resolve_qualified_type_paths(Decls0, Decls) :-
    decl_scope_tree(Decls0, Root),
    qualified_type_names([Root], Decls0, Names),
    maplist(resolve_qualified_type_decl([Root]), Decls0, Decls1),
    foldl(ensure_type_decl, Names, Decls1, Decls).

resolve_qualified_type_decl(Scopes, col_type(Ref, Column, Type0),
                            col_type(Ref, Column, Type)) :-
    !,
    resolve_qualified_type(Scopes, Type0, Type).
resolve_qualified_type_decl(Scopes, type_decl(Name, Specs0),
                            type_decl(Name, Specs)) :-
    !,
    resolve_qualified_type(Scopes, Specs0, Specs).
resolve_qualified_type_decl(Scopes, rel_template(Segments, Parameters0, Specs0),
                            rel_template(Segments, Parameters, Specs)) :-
    !,
    resolve_qualified_type(Scopes, Parameters0, Parameters),
    resolve_qualified_type(Scopes, Specs0, Specs).
resolve_qualified_type_decl(Scopes,
                            rel_template_enum(Segments, Parameters0, Variants0),
                            rel_template_enum(Segments, Parameters, Variants)) :-
    !,
    resolve_qualified_type(Scopes, Parameters0, Parameters),
    resolve_qualified_type(Scopes, Variants0, Variants).
resolve_qualified_type_decl(Scopes, enum_decl(Name, Variants0),
                            enum_decl(Name, Variants)) :-
    !,
    resolve_qualified_type(Scopes, Variants0, Variants).
resolve_qualified_type_decl(Scopes, sh_decl(Name, Inputs0, Outputs0, Template),
                            sh_decl(Name, Inputs, Outputs, Template)) :-
    !,
    resolve_qualified_type(Scopes, Inputs0, Inputs),
    resolve_qualified_type(Scopes, Outputs0, Outputs).
resolve_qualified_type_decl(_, Decl, Decl).

resolve_qualified_type(Scopes, type_path_application(Segments, Arguments0),
                       Type) :-
    !,
    (   resolve_path(Scopes, Segments, Name)
    ->  maplist(resolve_qualified_type(Scopes), Arguments0, Arguments),
        Type =.. [Name | Arguments]
    ;   throw(unsupported_construct(unresolvable_type_path(Segments)))
    ).
resolve_qualified_type(Scopes, type_path(Segments), Name) :-
    !,
    (   relation_id_path(Scopes, Segments, RelationName)
    ->  Name = id(RelationName)
    ;   resolve_path(Scopes, Segments, Name)
    ->  true
    ;   throw(unsupported_construct(unresolvable_type_path(Segments)))
    ).
resolve_qualified_type(Scopes, Type0, Type) :-
    compound(Type0),
    !,
    Type0 =.. [Functor | Args0],
    maplist(resolve_qualified_type(Scopes), Args0, Args),
    Type =.. [Functor | Args].
resolve_qualified_type(_, Type, Type).

% A terminal `.id` has relation-identity meaning only when its prefix already
% resolves to a declared relation. A mounted `source.span` remains an ordinary
% module path unless `source` itself names a declared relation.
relation_id_path(Scopes, Segments, Name) :-
    append(RelationSegments, [id], Segments),
    RelationSegments \== [],
    resolve_path(Scopes, RelationSegments, Name).

% The type_decl/2 mirror the parser (normalize_relation_value_decls/2) gives a
% bare name, at the same wrapper positions and no others. An id(Name) also
% needs Name's declared row shape, although it carries only its identity.
qualified_type_names(Scopes, Decls, Names) :-
    findall(Name,
            ( member(col_type(_, _, Surface), Decls),
              qualified_type_path(Surface, _),
              resolve_qualified_type(Scopes, Surface, Resolved),
              resolved_type_decl_name(Resolved, Name) ),
            Names0),
    sort(Names0, Names).

resolved_type_decl_name(id(Name), Name) :- !.
resolved_type_decl_name(Type, Name) :-
    column_element_type_name(Type, Name).

qualified_type_path(type_path(Segments), Segments) :- !.
qualified_type_path(type_path_application(Segments, _), Segments).
qualified_type_path(Type, Segments) :-
    compound(Type),
    Type =.. [_ | Args],
    member(Arg, Args),
    qualified_type_path(Arg, Segments).

ensure_type_decl(Name, Decls0, Decls) :-
    (   memberchk(type_decl(Name, _), Decls0)
    ->  Decls = Decls0
    ;   relation_type_decl(Name, Decls0, TypeDecl)
    ->  Decls = [TypeDecl | Decls0]
    ;   Decls = Decls0
    ).

relation_type_decl(Name, Decls, type_decl(Name, Specs)) :-
    once(member(col_type(Name/Arity, _, _), Decls)),
    findall(col(Column, Type), member(col_type(Name/Arity, Column, Type), Decls), Specs),
    length(Specs, Arity).

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

resolve_relation_paths(Decls, Terms0, Terms) :-
    decl_scope_tree(Decls, Root),
    maplist(resolve_rel_path_rule([Root]), Terms0, Terms).

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
    check_path_collisions(Paths),
    foldl(insert_path, Paths, node(file, none, []), Root).

% One path spelling two rels is a silent last-writer-wins in insert_path/3, so
% a mount landing on a name the file already declares is refused instead.
check_path_collisions([]).
check_path_collisions([_-_]).
check_path_collisions([SegmentsA-NameA, SegmentsB-NameB | Rest]) :-
    (   SegmentsA == SegmentsB,
        NameA \== NameB
    ->  throw(unsupported_construct(
                  mount_path_collision(SegmentsA, NameA, NameB)))
    ;   check_path_collisions([SegmentsB-NameB | Rest])
    ).

declared_path(Decls, Segments, Name) :-
    member(rel_path_decl(Name/_, Segments), Decls).
declared_path(Decls, Segments, Name) :-
    member(rel_template(Segments, _, _), Decls),
    atomic_list_concat(Segments, '__', Name).
declared_path(Decls, Segments, Name) :-
    member(rel_template_enum(Segments, _, _), Decls),
    atomic_list_concat(Segments, '__', Name).
declared_path(Decls, [Name], Name) :-
    declared_flat_names(Decls, Names),
    declared_path_names(Decls, PathNames),
    member(Name, Names),
    \+ memberchk(Name, PathNames).
% The mount graft. The mounted module's rel keeps its own flat NAME, so a
% reference through the alias resolves by identity and mints no new rel.
declared_path(Decls, [Alias | Segments], Name) :-
    member(mount_decl(Alias, _Mounted, _Owner, Paths), Decls),
    member(Segments-Name, Paths).

declared_flat_names(Decls, Names) :-
    findall(Name, declared_flat_name_raw(Decls, Name), Names0),
    sort(Names0, Names).

declared_path_names(Decls, Names) :-
    findall(Name, member(rel_path_decl(Name/_, _), Decls), Names0),
    sort(Names0, Names).

declared_flat_name_raw(Decls, Name) :- member(col_type(Name/_, _, _), Decls).
declared_flat_name_raw(Decls, Name) :- member(kind(Name/_, _), Decls).
declared_flat_name_raw(Decls, Name) :- member(keyed(Name/_, _), Decls).
declared_flat_name_raw(Decls, Name) :- member(keep(Name/_, _), Decls).
declared_flat_name_raw(Decls, Name) :- member(type_decl(Name, _), Decls).
declared_flat_name_raw(Decls, Name) :- member(enum_decl(Name, _), Decls).

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

% ── nested rels: the implicit leading parent reference ───────────────────────

%! expand_nested_parent_refs(+Decls0, +Rules0, -Decls, -Rules) is det.
%   No dotted decl = returned term-identical, so no flat program's shape moves.
expand_nested_parent_refs(Decls0, Rules0, Decls, Rules) :-
    (   member(rel_path_decl(_, _), Decls0)
    ->  nested_captures(Decls0, Captures),
        foldl(apply_nested_capture, Captures,
              program(Decls0, Rules0), program(Decls, Rules))
    ;   Decls = Decls0,
        Rules = Rules0
    ).

% Shallow paths first: a grandchild reads its parent's column list, so the
% parent's own capture has to be in the decls before the grandchild's runs.
nested_captures(Decls, Captures) :-
    decl_scope_tree(Decls, Root),
    findall(Depth-capture(Child, ParentName),
            ( member(rel_path_decl(Child/_, Segments), Decls),
              append(ParentSegments, [_Leaf], Segments),
              ParentSegments \== [],
              descend(Root, ParentSegments, ParentName),
              length(Segments, Depth) ),
            Keyed0),
    msort(Keyed0, Keyed),
    findall(Capture, member(_-Capture, Keyed), Captures).

apply_nested_capture(capture(Child, ParentName),
                     program(Decls0, Rules0), program(Decls, Rules)) :-
    child_capture_shape(Decls0, Child, ChildArity, ParentName, ParentSpecs),
    !,
    NewArity is ChildArity + 1,
    maplist(rename_capture_ref(Child/ChildArity, Child/NewArity),
            Decls0, Renamed),
    widen_captured_type_decl(Child, ParentName, Renamed, Widened),
    insert_parent_column(Child/NewArity, ParentName, Widened, Decls1),
    ensure_parent_type_decl(ParentName, ParentSpecs, Decls1, Decls),
    length(ParentSpecs, ParentArity),
    maplist(capture_rule(Child, ChildArity, ParentName, ParentArity),
            Rules0, Rules).
apply_nested_capture(_, Program, Program).

% An interior segment with no decl of its own captures nothing, and neither
% does a parent whose own columns are absent. A COLUMN-LESS child still does.
child_capture_shape(Decls, Child, ChildArity, ParentName, ParentSpecs) :-
    memberchk(rel_path_decl(Child/ChildArity, _), Decls),
    parent_specs(Decls, ParentName, ParentSpecs),
    findall(Column, member(col_type(Child/ChildArity, Column, _), Decls),
            ChildColumns),
    length(ChildColumns, ChildArity),
    (   memberchk(parent, ChildColumns)
    ->  throw(unsupported_construct(nested_parent_column_taken(Child)))
    ;   true
    ).

parent_specs(Decls, ParentName, Specs) :-
    memberchk(col_type(ParentName/ParentArity, _, _), Decls),
    findall(col(Column, Type),
            member(col_type(ParentName/ParentArity, Column, Type), Decls),
            Specs),
    length(Specs, ParentArity),
    ParentArity >= 1.

rename_capture_ref(Old, New, col_type(Old, Column, Type),
                   col_type(New, Column, Type)) :- !.
rename_capture_ref(Old, New, kind(Old, Kind), kind(New, Kind)) :- !.
rename_capture_ref(Old, New, keep(Old, Policy), keep(New, Policy)) :- !.
% Fork F-A: the captured ref claims position 1, so an author's own key
% positions all move one right.
rename_capture_ref(Old, New, keyed(Old, Positions), keyed(New, Shifted)) :-
    !,
    maplist(shift_key_position, Positions, Shifted).
rename_capture_ref(Old, New, rel_path_decl(Old, Segments),
                   rel_path_decl(New, Segments)) :- !.
rename_capture_ref(_, _, Decl, Decl).

% resolve_qualified_type_paths/2 may have materialized the child's row type
% before parent capture because another declaration refers to it. Keep that
% mirror at the same widened shape as the child's col_type declarations.
widen_captured_type_decl(Child, ParentName, Decls0, Decls) :-
    maplist(widen_captured_type_decl_(Child, ParentName), Decls0, Decls).

widen_captured_type_decl_(Child, ParentName, type_decl(Child, Specs),
                          type_decl(Child, [col(parent, ParentName) | Specs])) :-
    !.
widen_captured_type_decl_(_, _, Decl, Decl).

shift_key_position(Position, Shifted) :- Shifted is Position + 1.

% Column ORDER is decl order (analyze.pl rel_columns/5); a child with no
% column decl has no such slot and lands at its path carrier instead.
insert_parent_column(Ref, ParentName, Decls0, Decls) :-
    (   insert_parent_column_before(col_type(Ref, _, _), Ref, ParentName,
                                    Decls0, Inserted)
    ->  Decls = Inserted
    ;   insert_parent_column_before(rel_path_decl(Ref, _), Ref, ParentName,
                                    Decls0, Decls)
    ).

insert_parent_column_before(Anchor, Ref, ParentName, [Decl | Rest], Decls) :-
    (   subsumes_term(Anchor, Decl)
    ->  Decls = [col_type(Ref, parent, ParentName), Decl | Rest]
    ;   Decls = [Decl | More],
        insert_parent_column_before(Anchor, Ref, ParentName, Rest, More)
    ).

% column_storage/3 reads ref(Name) off the TYPE table, and the text door's own
% mirror ran at parse time, before this column existed.
ensure_parent_type_decl(ParentName, Specs, Decls0, Decls) :-
    (   memberchk(type_decl(ParentName, _), Decls0)
    ->  Decls = Decls0
    ;   Decls = [type_decl(ParentName, Specs) | Decls0]
    ).

capture_rule(Child, ChildArity, ParentName, ParentArity, Rule0, Rule) :-
    (   Rule0 = (Head0 <- Body0)
    ->  capture_arrow(Child, ChildArity, ParentName, ParentArity,
                      Head0, Body0, Head, Body),
        Rule = (Head <- Body)
    ;   Rule0 = (Head0 <+ Body0)
    ->  capture_arrow(Child, ChildArity, ParentName, ParentArity,
                      Head0, Body0, Head, Body),
        Rule = (Head <+ Body)
    ;   Rule = Rule0
    ).

capture_arrow(Child, ChildArity, ParentName, ParentArity, Head0, Body0,
              Head, Body) :-
    capture_head(Child, ChildArity, ParentName, ParentArity, Body0,
                 Head0, Head),
    capture_body(Child, ChildArity, Body0, Body).

% THE CONTRIBUTION RULE. A head short by one resolves its parent ref by
% natural join: the body's own parent atom becomes the leading head argument.
capture_head(Child, ChildArity, ParentName, ParentArity, Body, Head0, Head) :-
    (   capture_target_atom(Child, ChildArity, Head0, Args)
    ->  body_parent_term(ParentName, ParentArity, Body, Child, ParentTerm),
        Head =.. [Child, ParentTerm | Args]
    ;   Head = Head0
    ).

% A zero-column child is the bare ATOM, so `=..` is the only spelling that
% reaches both shapes.
capture_target_atom(Child, 0, Head, []) :-
    !,
    Head == Child.
capture_target_atom(Child, ChildArity, Head, Args) :-
    nonvar(Head),
    compound(Head),
    functor(Head, Child, ChildArity),
    Head =.. [_ | Args].

% lower.pl:bind_reference_target_identity/6 binds a whole body atom to its
% alias's `__id`, so a head argument that IS that atom projects the endpoint.
body_parent_term(ParentName, ParentArity, Body, Child, ParentTerm) :-
    conjunction_goals(Body, Goals),
    parent_atoms(Goals, ParentName, ParentArity, Atoms0),
    sort(Atoms0, Atoms),
    (   Atoms == []
    ->  throw(unsupported_construct(nested_parent_unbound(Child)))
    ;   Atoms = [ParentTerm]
    ->  true
    ;   throw(unsupported_construct(nested_parent_ambiguous(Child)))
    ).

% findall/3 COPIES, which hands the head a fresh parent term and costs the
% rule its natural join; the collection has to keep the body's own variables.
parent_atoms([], _, _, []).
parent_atoms([Goal | Rest], ParentName, ParentArity, Atoms) :-
    (   positive_parent_atom(ParentName, ParentArity, Goal, Atom)
    ->  Atoms = [Atom | More]
    ;   Atoms = More
    ),
    parent_atoms(Rest, ParentName, ParentArity, More).

positive_parent_atom(ParentName, ParentArity, Goal, Atom) :-
    nonvar(Goal),
    (   Goal = latest(Inner)
    ->  Atom = Inner
    ;   Atom = Goal
    ),
    nonvar(Atom),
    compound(Atom),
    functor(Atom, ParentName, ParentArity).

% A zero-column child is a bare ATOM, and an atom is a legal DATA value, so a
% whole-term walk would rewrite `flag` used as a text or enum value.
capture_body(Child, 0, Body0, Body) :-
    !,
    conjunction_goals(Body0, Goals0),
    maplist(capture_atom_goal(Child), Goals0, Goals),
    goals_conjunction(Goals, Body).
% A body atom short by one reads across every parent partition, so each
% occurrence takes its own fresh leading variable.
capture_body(Child, ChildArity, Term0, Term) :-
    (   var(Term0)
    ->  Term = Term0
    ;   compound(Term0),
        functor(Term0, Child, ChildArity)
    ->  Term0 =.. [_ | Args0],
        maplist(capture_body(Child, ChildArity), Args0, Args),
        Term =.. [Child, _Partition | Args]
    ;   compound(Term0)
    ->  Term0 =.. [Functor | Args0],
        maplist(capture_body(Child, ChildArity), Args0, Args),
        Term =.. [Functor | Args]
    ;   Term = Term0
    ).

capture_atom_goal(Child, Goal0, Goal) :-
    (   Goal0 == Child
    ->  Goal =.. [Child, _Partition]
    ;   nonvar(Goal0),
        compound(Goal0),
        Goal0 =.. [Wrapper, Inner],
        goal_wrapper(Wrapper),
        Inner == Child
    ->  Partitioned =.. [Child, _Partition],
        Goal =.. [Wrapper, Partitioned]
    ;   Goal = Goal0
    ).

goal_wrapper(not).
goal_wrapper(latest).
goal_wrapper(pre).
goal_wrapper(finalize).
goal_wrapper(next).

expand_dot_rule(Decls, Rule0, Rule) :-
    ( contains_dot_get(Rule0)
    -> desugar_dot_rule(Decls, Rule0, Rule)
    ;  Rule = Rule0
    ).

desugar_dot_rule(Decls, (Head0 <- Body0), (Head <- Body)) :- !,
    desugar_head_and_body(Decls, Head0, Body0, Head, Body).
desugar_dot_rule(Decls, (Head0 <+ Body0), (Head <+ Body)) :- !,
    desugar_head_and_body(Decls, Head0, Body0, Head, Body).
desugar_dot_rule(_, Rule, Rule).

desugar_head_and_body(Decls, Head0, Body0, Head, Body) :-
    conjunction_goals(Body0, Goals0),
    bound_body_vars(Goals0, BoundVars),
    maplist(rewrite_goal(Decls, BoundVars, Goals0), Goals0, GoalLists),
    append(GoalLists, BodyGoals),
    rewrite_head(Decls, Goals0, Head0, BoundVars, Head, HeadDecodes),
    append(BodyGoals, HeadDecodes, FinalGoals),
    goals_conjunction(FinalGoals, Body).

% ── head arguments ───────────────────────────────────────────────────────────
% A head dot chain is ruled IN: `dcoord(FileRec.at.name, S, E) <- span(...)`.
% The receiver still has to be bound by the BODY, which is why the head is
% rewritten against the same BoundVars set the body goals used.

rewrite_head(Decls, Goals, Head0, BoundVars, Head, Decodes) :-
    ( compound(Head0)
    -> Head0 =.. [Name | Args0],
       foldl(replace_dot_gets_arg(Decls, Goals, BoundVars), Args0, Args, [], Decodes),
       Head =.. [Name | Args]
    ;  Head = Head0, Decodes = []
    ).

% ── goal rewriting ───────────────────────────────────────────────────────────

rewrite_goal(_, _, _, Goal, [Goal]) :-
    \+ contains_dot_get(Goal),
    !.
rewrite_goal(_, _, _, Goal, _) :-
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
rewrite_goal(Decls, BoundVars, Goals, (Lhs := Rhs), [decode(Root, Pattern)]) :-
    nonvar(Rhs),
    Rhs = dot_get(_, _),
    \+ contains_dot_get(Lhs),
    !,
    dot_chain_parts(Rhs, Root, Fields),
    check_dot_receiver(BoundVars, Root, Rhs),
    dot_fields_pattern(Decls, Goals, Root, Fields, Lhs, Pattern).
rewrite_goal(Decls, BoundVars, Goals, Goal0, GoalList) :-
    replace_dot_gets(Decls, Goals, Goal0, BoundVars, Goal, Decodes),
    ( plain_relation_goal(Goal)
    -> GoalList = [Goal | Decodes]
    ;  append(Decodes, [Goal], GoalList)
    ).

replace_dot_gets(_, _, Term, _, Term, []) :-
    var(Term),
    !.
replace_dot_gets(Decls, Goals, Term, BoundVars, Leaf, [decode(Root, Pattern)]) :-
    nonvar(Term),
    Term = dot_get(_, _),
    !,
    dot_chain_parts(Term, Root, Fields),
    check_dot_receiver(BoundVars, Root, Term),
    dot_fields_pattern(Decls, Goals, Root, Fields, Leaf, Pattern).
replace_dot_gets(Decls, Goals, Term, BoundVars, Out, Decodes) :-
    compound(Term),
    !,
    Term =.. [Functor | Args0],
    foldl(replace_dot_gets_arg(Decls, Goals, BoundVars), Args0, Args, [], Decodes),
    Out =.. [Functor | Args].
replace_dot_gets(_, _, Term, _, Term, []).

replace_dot_gets_arg(Decls, Goals, BoundVars, Arg0, Arg, Acc, Decodes) :-
    replace_dot_gets(Decls, Goals, Arg0, BoundVars, Arg, ArgDecodes),
    append(Acc, ArgDecodes, Decodes).

% `FileRec.revision.id` reads File's stored endpoint once. The synthesized
% decode joins File's dictionary but never follows Revision.
dot_fields_pattern(Decls, Goals, Root, Fields, Leaf, Pattern) :-
    ( relation_id_member_path(Decls, Goals, Root, Fields, Field)
    -> fields_pattern([Field], Leaf, Pattern)
    ;  fields_pattern(Fields, Leaf, Pattern)
    ).

relation_id_member_path(Decls, Goals, Root, Fields, Field) :-
    Fields = [Field, id],
    receiver_relation_type(Decls, Goals, Root, OwnerType),
    type_definitions(Decls, Types),
    type_definition(Types, OwnerType, Columns, ColumnTypes),
    nth1(Position, Columns, Field),
    nth1(Position, ColumnTypes, TargetType),
    declared_type_name(Types, TargetType).

receiver_relation_type(Decls, Goals, Root, OwnerType) :-
    type_definitions(Decls, Types),
    member(Goal, Goals),
    compound(Goal),
    functor(Goal, Name, Arity),
    relation_columns_and_types(Decls, Types, Name/Arity, _Columns, ColumnTypes),
    nth1(Position, ColumnTypes, OwnerType),
    arg(Position, Goal, Argument),
    Argument == Root,
    declared_type_name(Types, OwnerType),
    !.

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
binding_positions(pre(Atom, _), [Atom]) :- !.
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
