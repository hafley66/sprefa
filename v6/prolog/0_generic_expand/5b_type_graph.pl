% Canonical semantic rows project into ephemeral compiler-round graph sources.
% Specialized rows remain the authority and runtime planning sees none of the
% node, edge, or path relations below.

%! type_graph_compiler_source_rows(+Decls, +SemanticRows, +Relations, -Rows)
type_graph_compiler_source_rows(Decls, SemanticRows, Relations, Rows) :-
    requested_type_graph_nodes(Decls, SemanticRows, Relations, NodeRows),
    requested_type_graph_edges(Decls, SemanticRows, Relations, EdgeRows),
    requested_type_graph_paths(Decls, SemanticRows, Relations, PathRows),
    append([NodeRows, EdgeRows, PathRows], Rows0),
    sort(Rows0, Rows).

requested_type_graph_nodes(Decls, SemanticRows, Relations, Rows) :-
    memberchk(compiler_relation(type__node/3, _, _), Relations),
    !,
    type_graph_nodes(Decls, SemanticRows, Nodes),
    findall(type__node(Id, Kind, Label),
            member(type_node(Id, Kind, Label), Nodes),
            Rows).
requested_type_graph_nodes(_, _, _, []).

requested_type_graph_edges(Decls, SemanticRows, Relations, Rows) :-
    memberchk(compiler_relation(type__edge/6, _, _), Relations),
    !,
    type_graph_edges(Decls, SemanticRows, Edges),
    findall(type__edge(Id, Owner, Role, Position, Label, Target),
            member(type_edge(Id, Owner, Role, Position, Label, Target), Edges),
            Rows).
requested_type_graph_edges(_, _, _, []).

requested_type_graph_paths(Decls, SemanticRows, Relations, Rows) :-
    memberchk(compiler_relation(type__path/2, _, _), Relations),
    !,
    type_graph_paths(Decls, SemanticRows, Paths),
    findall(type__path(Id, Path), member(type_path(Id, Path), Paths), Rows).
requested_type_graph_paths(_, _, _, []).

%! type_graph_nodes(+Decls, +SemanticRows, -Nodes) is det.
type_graph_nodes(Decls, SemanticRows, Nodes) :-
    findall(type_node(Id, Kind, Label),
            type_graph_direct_node(Decls, SemanticRows, Id, Kind, Label),
            DirectNodes),
    type_graph_edges(Decls, SemanticRows, Edges),
    findall(type_node(Id, Kind, Label),
            ( member(type_edge(_, Owner, _, _, _, Target), Edges),
              member(Id, [Owner, Target]),
              type_graph_structural_node(SemanticRows, Id, Kind, Label) ),
            EndpointNodes),
    append(DirectNodes, EndpointNodes, Nodes0),
    sort(Nodes0, Nodes),
    validate_type_graph_nodes(Nodes).

type_graph_direct_node(_, _, primitive(Name), primitive, Name) :-
    semantic_primitive(Name).
type_graph_direct_node(_, Rows, Id, declaration, Name) :-
    member(declaration(Id, _, Name, _, _), Rows).
type_graph_direct_node(_, Rows, Id, parameter, Name) :-
    member(parameter(Id, _, _, Name), Rows).
type_graph_direct_node(_, Rows, Id, member, Name) :-
    member(member(Id, _, _, Name, _), Rows).
type_graph_direct_node(_, Rows, Id, application, '') :-
    member(application(Id, _), Rows).
type_graph_direct_node(_, Rows, Id, argument, '') :-
    member(argument(Id, _, _, _), Rows).
type_graph_direct_node(_, Rows, Id, anonymous, Path) :-
    member(Id, Rows),
    Id = anonymous(_, Path, _).
type_graph_direct_node(_, Rows, derivation(Materialized, Source), derivation,
                       '') :-
    member(derived_from(Materialized, Source), Rows).
type_graph_direct_node(Decls, _, Id, annotation_site, Site) :-
    type_graph_annotation_site(Decls, Id, _, Site, _, _).

type_graph_structural_node(Rows, Id, Kind, Label) :-
    type_graph_direct_node([], Rows, Id, Kind, Label),
    !.
type_graph_structural_node(_, named(_, _, Name), declaration, Name) :- !.
type_graph_structural_node(_, primitive(Name), primitive, Name) :- !.
type_graph_structural_node(_, application(_, _), application, '') :- !.
type_graph_structural_node(_, parameter(_, _, Name), parameter, Name) :- !.
type_graph_structural_node(_, anonymous(_, Path, _), anonymous, Path) :- !.
type_graph_structural_node(_, member(_, _, Name), member, Name) :- !.
type_graph_structural_node(_, argument(_, _), argument, '') :- !.
type_graph_structural_node(_, derivation(_, _), derivation, '') :- !.
type_graph_structural_node(_, annotation_site(_, Site, _), annotation_site,
                           Site).

validate_type_graph_nodes(Nodes) :-
    findall(Id-type_node(Id, Kind, Label),
            member(type_node(Id, Kind, Label), Nodes),
            Pairs0),
    keysort(Pairs0, Pairs),
    group_pairs_by_key(Pairs, Groups),
    maplist(validate_type_graph_node_group, Groups).

validate_type_graph_node_group(_-[type_node(_, _, _)]) :- !.
validate_type_graph_node_group(Id-Rows) :-
    sort(Rows, Distinct),
    ( Distinct = [_]
    -> true
    ;  throw(unsupported_construct(canonical_type_node_conflict(Id, Distinct)))
    ).

%! type_graph_edges(+Decls, +SemanticRows, -Edges) is det.
type_graph_edges(Decls, SemanticRows, Edges) :-
    findall(type_edge(Id, Owner, Role, Position, Label, Target),
            type_graph_edge(Decls, SemanticRows, Id, Owner, Role, Position,
                            Label, Target),
            Edges0),
    sort(Edges0, Edges),
    validate_type_graph_edges(Edges).

type_graph_edge(_, Rows, MemberId, Owner, Role, Position, Name, Target) :-
    member(member(MemberId, Owner, Position, Name, type_ref(TypeRef)), Rows),
    projection_type_ref_target(TypeRef, Target),
    member_edge_role(Rows, Owner, Role).
type_graph_edge(_, Rows, ArgumentId, Application, argument, Position, '',
                Target) :-
    member(argument(ArgumentId, Application, Position, _), Rows),
    Application = application(_, Arguments),
    nth1(Position, Arguments, Target).
type_graph_edge(_, Rows, constructor_edge(Application), Application,
                constructor, 0, '', Constructor) :-
    member(application(Application, Constructor), Rows).
type_graph_edge(_, Rows, derivation(Materialized, Source), Materialized,
                materializes, 0, '', Source) :-
    member(derived_from(Materialized, Source), Rows).
type_graph_edge(Decls, Rows, nested(Owner, Name, Child), Owner, nested, 0,
                Name, Child) :-
    nested_type_projection(Decls, Rows, Owner, Name, Child, _).
type_graph_edge(Decls, _, annotation_edge(SiteId), Member, annotation,
                Ordinal, Annotator, SiteId) :-
    type_graph_annotation_site(Decls, SiteId, Member, _, Ordinal, Annotator).

member_edge_role(Rows, Owner, variant) :-
    member(declaration(Owner, _, _, enum, _), Rows),
    !.
member_edge_role(_, _, member).

type_graph_annotation_site(Decls, annotation_site(Member, Site, Ordinal),
                           Member, Site, Ordinal, Annotator) :-
    member(compiler_annotation_evidence(Evidence), Decls),
    member(annotation_evidence(Member, Site, Ordinal, _, _, _, AnnotationRow),
           Evidence),
    AnnotationRow =.. [AnnotatorName | _],
    semantic_decl_id(Decls, relation, AnnotatorName, Annotator).

validate_type_graph_edges(Edges) :-
    validate_type_graph_edge_ids(Edges),
    validate_type_graph_edge_keys(Edges).

validate_type_graph_edge_ids(Edges) :-
    findall(Id-Edge,
            ( member(Edge, Edges), Edge = type_edge(Id, _, _, _, _, _) ),
            Pairs0),
    keysort(Pairs0, Pairs),
    group_pairs_by_key(Pairs, Groups),
    maplist(validate_type_graph_edge_id_group, Groups).

validate_type_graph_edge_id_group(_-[type_edge(_, _, _, _, _, _)]) :- !.
validate_type_graph_edge_id_group(Id-Rows) :-
    sort(Rows, Distinct),
    ( Distinct = [_]
    -> true
    ;  throw(unsupported_construct(canonical_type_edge_conflict(Id, Distinct)))
    ).

validate_type_graph_edge_keys(Edges) :-
    findall(key(Owner, Role, Position, Label)-Edge,
            ( member(Edge, Edges),
              Edge = type_edge(_, Owner, Role, Position, Label, _) ),
            Pairs0),
    keysort(Pairs0, Pairs),
    group_pairs_by_key(Pairs, Groups),
    maplist(validate_type_graph_edge_key_group, Groups).

validate_type_graph_edge_key_group(_-[type_edge(_, _, _, _, _, _)]) :- !.
validate_type_graph_edge_key_group(Key-Rows) :-
    sort(Rows, Distinct),
    ( Distinct = [_]
    -> true
    ;  throw(unsupported_construct(canonical_type_edge_key_conflict(Key,
                                                                   Distinct)))
    ).

%! type_graph_paths(+Decls, +SemanticRows, -Paths) is det.
type_graph_paths(Decls, SemanticRows, Paths) :-
    findall(type_path(Id, Path),
            type_graph_path(Decls, SemanticRows, Id, Path),
            Paths0),
    sort(Paths0, Paths).

type_graph_path(Decls, Rows, Id, Path) :-
    declared_path(Decls, Path, Name),
    member(declaration(Id, _, Name, _, _), Rows).
type_graph_path(_, Rows, anonymous(Owner, Path, Shape), Path) :-
    member(anonymous(Owner, Path, Shape), Rows).
type_graph_path(_, Rows, Materialized, Path) :-
    member(derived_from(Materialized, anonymous(Owner, Path, Shape)), Rows),
    member(anonymous(Owner, Path, Shape), Rows).
type_graph_path(Decls, _, SiteId, Site) :-
    type_graph_annotation_site(Decls, SiteId, _, Site, _, _).
