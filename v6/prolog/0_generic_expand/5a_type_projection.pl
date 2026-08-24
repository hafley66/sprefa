% A type projection has one canonical target:
%
%   (OwnerType, Name) -> TargetType
%
% Members and explicitly nested relations contribute to the same view.  The
% view stays compiler-internal for this experiment, so target emitters and the
% serialized semantic row contract do not move.

%! type_projection_targets(+Decls, -Targets) is det.
%  Targets is the deduplicated canonical view after semantic type freeze:
%
%    type_projection(OwnerType, Name, TargetType)
type_projection_targets(Decls, Targets) :-
    semantic_rows_in_decls(Decls, Rows),
    type_projection_edges(Decls, Rows, Edges),
    findall(type_projection(Owner, Name, Target),
            member(type_projection_edge(Owner, Name, Target, _), Edges),
            Unsorted),
    sort(Unsorted, Targets).

%! validate_type_projection_targets(+Decls, +Rows) is det.
%  Every (OwnerType, Name) group must contain one distinct TargetType.
validate_type_projection_targets(Decls, Rows) :-
    type_projection_edges(Decls, Rows, Edges),
    findall((Owner-Name)-Edge,
            ( member(Edge, Edges),
              Edge = type_projection_edge(Owner, Name, _, _) ),
            Pairs0),
    keysort(Pairs0, Pairs),
    group_pairs_by_key(Pairs, Groups),
    maplist(validate_type_projection_group, Groups).

validate_type_projection_group((Owner-Name)-Edges) :-
    (   member(type_projection_edge(_, _, _, nested(_)), Edges)
    ->  findall(Target,
                member(type_projection_edge(_, _, Target, _), Edges),
                Targets0),
        sort(Targets0, Targets),
        (   Targets = [_]
        ->  true
        ;   throw(unsupported_construct(
                      ambiguous_type_projection(Owner, Name, Targets)))
        )
    ;   true
    ).

type_projection_edges(Decls, Rows, Edges) :-
    findall(type_projection_edge(Owner, Name, Target, member(MemberId)),
            ( member(member(MemberId, Owner, _, Name, type_ref(TypeRef)), Rows),
              projection_type_ref_target(TypeRef, Target),
              projection_owner_is_type(Owner, Rows) ),
            MemberEdges),
    findall(type_projection_edge(Owner, Name, Child,
                                 nested(ChildPath)),
            nested_type_projection(Decls, Rows, Owner, Name, Child,
                                   ChildPath),
            NestedEdges),
    append(MemberEdges, NestedEdges, Edges).

projection_type_ref_target(declaration(Id), Id) :- !.
projection_type_ref_target(application(Id), Id) :- !.
projection_type_ref_target(parameter(Id), Id) :- !.
projection_type_ref_target(Target, Target).

projection_owner_is_type(Owner, Rows) :-
    member(declaration(Owner, _, _, Kind, _), Rows),
    memberchk(Kind, [relation, enum, interface]).

nested_type_projection(Decls, Rows, Owner, Name, Child, ChildPath) :-
    member(rel_path_decl(ChildName/_, ChildPath), Decls),
    append(ParentPath, [Name], ChildPath),
    ParentPath \== [],
    declared_path(Decls, ParentPath, ParentName),
    member(declaration(Owner, _, ParentName, relation, _), Rows),
    member(declaration(Child, _, ChildName, relation, _), Rows).
