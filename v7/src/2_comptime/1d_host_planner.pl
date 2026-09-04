:- module(dl7_host_planner,
          [ validate_hosted_relations/4,
            erase_host_planning_rows/7
          ]).

%% validate_hosted_relations(+Graph, +Relations, +CompilerFacts,
%%                           -Diagnostics) is det.
%
% Hosted and HostPort are ordinary prelude relations during compiler
% evaluation. Once their rows close, validate the external operator shape
% against the same :/4 edges that define the callable relation.
validate_hosted_relations(Graph, Relations, CompilerFacts, Diagnostics) :-
    host_schema(Graph, Schema),
    host_rows(Schema, CompilerFacts, HostedRows, PortRows),
    host_relation_ids(HostedRows, PortRows, RelationIds),
    validate_host_relation_ids(
        RelationIds, Schema, Graph, Relations, HostedRows, PortRows,
        Diagnostics0),
    sort(Diagnostics0, Diagnostics).

host_schema(root_graph(Nodes, Edges),
            host_schema(HostedIds, HostPortIds, InputIds, OutputIds)) :-
    named_module_targets(Nodes, Edges, 'Hosted', HostedIds),
    named_module_targets(Nodes, Edges, 'HostPort', HostPortIds),
    named_module_targets(Nodes, Edges, 'Input', InputIds),
    named_module_targets(Nodes, Edges, 'Output', OutputIds).

named_module_targets(Nodes, Edges, Name, Targets) :-
    findall(Target,
            ( member(module(Module), Nodes),
              member(':'(Module, Name, ref(Target), _), Edges)
            ),
            Targets0),
    sort(Targets0, Targets).

host_rows(host_schema(HostedIds, HostPortIds, _, _), CompilerFacts,
          HostedRows, PortRows) :-
    findall(hosted(Relation, Implementation),
            ( member(call(ref(Hosted),
                          [ref(Relation), ref(Implementation)]),
                     CompilerFacts),
              memberchk(Hosted, HostedIds)
            ),
            HostedRows0),
    sort(HostedRows0, HostedRows),
    findall(host_port(Relation, Label, Direction),
            ( member(call(ref(HostPort),
                          [ref(Relation), const(Label), ref(Direction)]),
                     CompilerFacts),
              memberchk(HostPort, HostPortIds)
            ),
            PortRows0),
    sort(PortRows0, PortRows).

host_relation_ids(HostedRows, PortRows, RelationIds) :-
    findall(Relation,
            ( member(hosted(Relation, _), HostedRows)
            ; member(host_port(Relation, _, _), PortRows)
            ),
            RelationIds0),
    sort(RelationIds0, RelationIds).

validate_host_relation_ids([], _, _, _, _, _, []).
validate_host_relation_ids(
    [Relation | Relations], Schema, Graph, DeclaredRelations,
    HostedRows, PortRows, Diagnostics) :-
    validate_host_relation(
        Relation, Schema, Graph, DeclaredRelations,
        HostedRows, PortRows, OwnDiagnostics),
    validate_host_relation_ids(
        Relations, Schema, Graph, DeclaredRelations,
        HostedRows, PortRows, RestDiagnostics),
    append(OwnDiagnostics, RestDiagnostics, Diagnostics).

validate_host_relation(
    Relation, Schema, root_graph(_, Edges), DeclaredRelations,
    HostedRows, PortRows, Diagnostics) :-
    findall(Implementation,
            member(hosted(Relation, Implementation), HostedRows),
            Implementations),
    length(Implementations, ImplementationCount),
    implementation_diagnostics(
        Relation, ImplementationCount, ImplementationDiagnostics),
    relation_declaration_diagnostics(
        Relation, DeclaredRelations, DeclarationDiagnostics),
    findall(edge(Label, Index),
            member(':'(Relation, Label, _, Index), Edges),
            RelationEdges0),
    sort(RelationEdges0, RelationEdges),
    relation_port_diagnostics(
        Relation, RelationEdges, PortRows, PortDiagnostics),
    unknown_port_diagnostics(
        Relation, RelationEdges, PortRows, UnknownDiagnostics),
    direction_diagnostics(
        Relation, Schema, PortRows, DirectionDiagnostics),
    append([ ImplementationDiagnostics,
             DeclarationDiagnostics,
             PortDiagnostics,
             UnknownDiagnostics,
             DirectionDiagnostics
           ], Diagnostics).

implementation_diagnostics(_, 1, []) :- !.
implementation_diagnostics(Relation, Count,
                           [diagnostic(
                                host, none,
                                hosted_implementation_count(Relation,
                                                            Count))]).

relation_declaration_diagnostics(Relation, Relations, []) :-
    memberchk(relation(ref(Relation), _, _), Relations),
    !.
relation_declaration_diagnostics(
    Relation, _,
    [diagnostic(host, none, hosted_relation_not_declared(Relation))]).

relation_port_diagnostics(_, [], _, []).
relation_port_diagnostics(
    Relation, [edge(Label, Index) | Edges], PortRows, Diagnostics) :-
    findall(Direction,
            member(host_port(Relation, Label, Direction), PortRows),
            Directions),
    length(Directions, Count),
    port_count_diagnostics(
        Relation, Label, Index, Count, OwnDiagnostics),
    relation_port_diagnostics(
        Relation, Edges, PortRows, RestDiagnostics),
    append(OwnDiagnostics, RestDiagnostics, Diagnostics).

port_count_diagnostics(_, _, _, 1, []) :- !.
port_count_diagnostics(
    Relation, Label, Index, Count,
    [diagnostic(host, none,
                hosted_port_direction_count(Relation, Label, Index,
                                            Count))]).

unknown_port_diagnostics(Relation, RelationEdges, PortRows, Diagnostics) :-
    findall(diagnostic(host, none,
                       hosted_port_unknown_edge(Relation, Label)),
            ( member(host_port(Relation, Label, _), PortRows),
              \+ memberchk(edge(Label, _), RelationEdges)
            ),
            Diagnostics).

direction_diagnostics(
    Relation, host_schema(_, _, InputIds, OutputIds), PortRows,
    Diagnostics) :-
    append(InputIds, OutputIds, Directions),
    findall(diagnostic(host, none,
                       hosted_port_invalid_direction(
                           Relation, Label, Direction)),
            ( member(host_port(Relation, Label, Direction), PortRows),
              \+ memberchk(Direction, Directions)
            ),
            Diagnostics).

%% erase_host_planning_rows(+Graph, +Relations0, +Seeds0, +Rules0,
%%                          -Relations, -Seeds, -Rules) is det.
%
% The closed CompilerFacts retain Hosted and HostPort tuples. Their source
% relations, source rows, and compiler-only consumers do not enter the emitted
% runtime program.
erase_host_planning_rows(
    Graph, Relations0, Seeds0, Rules0, Relations, Seeds, Rules) :-
    host_schema(Graph, host_schema(HostedIds, HostPortIds, _, _)),
    append(HostedIds, HostPortIds, PlanningIds0),
    sort(PlanningIds0, PlanningIds),
    exclude(relation_targets_any(PlanningIds), Relations0, Relations),
    exclude(call_targets_any(PlanningIds), Seeds0, Seeds),
    exclude(rule_uses_any(PlanningIds), Rules0, Rules).

relation_targets_any(Ids, relation(ref(Relation), _, _)) :-
    memberchk(Relation, Ids).

call_targets_any(Ids, call(ref(Relation), _)) :-
    memberchk(Relation, Ids).

rule_uses_any(Ids, rule(call(ref(Relation), _), _)) :-
    memberchk(Relation, Ids),
    !.
rule_uses_any(Ids, rule(_, Body)) :-
    member(checked_goal(_, call(ref(Relation), _)), Body),
    memberchk(Relation, Ids).
