:- module(dl7_module_resolver,
          [ resolve_path/6,
            check_visible_name_collisions/3,
            check_module_cycles/2
          ]).

:- use_module(library(error), [must_be/2]).
:- use_module(library(ugraphs),
              [ neighbors/3,
                vertices_edges_to_ugraph/3
              ]).

%% resolve_path(+StartOwner, +Segments, +Edges,
%%              -Target, -Proof, -Diagnostics) is det.
%
% Walk owner/name edges one segment at a time. Each proof row records the
% concrete edge used by that step. Missing and ambiguous segments stop at the
% first unresolved position.
resolve_path(StartOwner, Segments, Edges, Target, Proof, Diagnostics) :-
    must_be(ground, StartOwner),
    must_be(list, Segments),
    must_be(ground, Segments),
    must_be(list, Edges),
    must_be(ground, Edges),
    resolve_path_segments(Segments, 0, StartOwner, Edges,
                          [], Target, Proof, Diagnostics).

resolve_path_segments([], _, Current, _, Proof, Current, Proof, []).
resolve_path_segments([Name | Names], SegmentIndex, Current, Edges,
                      Proof0, Target, Proof, Diagnostics) :-
    path_candidates(Current, Name, Edges, Candidates),
    continue_path_candidates(Candidates, Name, Names, SegmentIndex,
                             Current, Edges, Proof0,
                             Target, Proof, Diagnostics).

continue_path_candidates(
    [path_step(Current, Name, Next, EdgeIndex)], _, Names, SegmentIndex,
    Current, Edges, Proof0, Target, Proof, Diagnostics) :-
    !,
    append(Proof0, [path_step(Current, Name, Next, EdgeIndex)], Proof1),
    NextSegmentIndex is SegmentIndex + 1,
    resolve_path_segments(Names, NextSegmentIndex, Next, Edges,
                          Proof1, Target, Proof, Diagnostics).
continue_path_candidates([], Name, _, SegmentIndex, Current, _, Proof,
                         none, Proof,
                         [diagnostic(
                              resolve_path, segment(SegmentIndex),
                              missing_path_segment(Current, Name))]) :-
    !.
continue_path_candidates(Candidates, Name, _, SegmentIndex, Current, _, Proof,
                         none, Proof,
                         [diagnostic(
                              resolve_path, segment(SegmentIndex),
                              ambiguous_path_segment(
                                  Current, Name, Candidates))]).

path_candidates(Owner, Name, Edges, Candidates) :-
    findall(
        path_step(Owner, Name, Target, Index),
        ( member(Edge, Edges),
          graph_edge(Edge, Owner, Name, TargetTerm, Index),
          traversable_target(TargetTerm, Target)
        ),
        Candidates0),
    sort(Candidates0, Candidates).

graph_edge(pending_edge(Owner, Name, Target, Index),
           Owner, Name, Target, Index).
graph_edge(':'(Owner, Name, Target, Index),
           Owner, Name, Target, Index).

traversable_target(target(Target), Target).
traversable_target(ref(Target), Target).
traversable_target(const(Value), const(Value)).
traversable_target(name(Owner, Name), name(Owner, Name)).

%% check_visible_name_collisions(+LocalEdges, +Imports, -Diagnostics) is det.
%
% Local duplicate names are invalid. Imported module aliases may repeat only
% when every row names the same imported module. A local edge shadows an
% imported alias and therefore does not create a visibility diagnostic.
check_visible_name_collisions(LocalEdges, Imports, Diagnostics) :-
    must_be(ground, LocalEdges),
    must_be(ground, Imports),
    duplicate_local_name_diagnostics(LocalEdges, LocalDiagnostics),
    imported_alias_diagnostics(Imports, ImportDiagnostics),
    append(LocalDiagnostics, ImportDiagnostics, Diagnostics0),
    sort(Diagnostics0, Diagnostics).

duplicate_local_name_diagnostics(Edges, Diagnostics) :-
    findall(
        diagnostic(module, Owner,
                   duplicate_local_name(Owner, Name, Entries)),
        duplicate_local_name(Edges, Owner, Name, Entries),
        Diagnostics).

duplicate_local_name(Edges, Owner, Name, Entries) :-
    setof(Owner-Name,
          Target^Index^Edge^(
              member(Edge, Edges),
              graph_edge(Edge, Owner, Name, Target, Index)),
          OwnerNames),
    member(Owner-Name, OwnerNames),
    findall(edge(Target, Index),
            ( member(Edge, Edges),
              graph_edge(Edge, Owner, Name, Target, Index)
            ),
            Entries),
    length(Entries, EntryCount),
    EntryCount > 1.

imported_alias_diagnostics(Imports, Diagnostics) :-
    findall(Diagnostic,
            imported_alias_diagnostic(Imports, Diagnostic),
            Diagnostics).

imported_alias_diagnostic(
    Imports,
    diagnostic(module, Importer,
               ambiguous_import_alias(Importer, Alias, ImportedModules))) :-
    import_alias_group(Imports, Importer, Alias, Rows),
    findall(Imported,
            member(module_import(Importer, Imported, Alias), Rows),
            Imported0),
    sort(Imported0, ImportedModules),
    length(ImportedModules, ModuleCount),
    ModuleCount > 1.
imported_alias_diagnostic(
    Imports,
    diagnostic(module, Importer,
               duplicate_module_import(Importer, Imported, Alias))) :-
    import_alias_group(Imports, Importer, Alias, Rows),
    sort(Rows, [module_import(Importer, Imported, Alias)]),
    length(Rows, RowCount),
    RowCount > 1.

import_alias_group(Imports, Importer, Alias, Rows) :-
    setof(Importer-Alias,
          Imported^member(module_import(Importer, Imported, Alias), Imports),
          ImporterAliases),
    member(Importer-Alias, ImporterAliases),
    findall(module_import(Importer, Imported, Alias),
            member(module_import(Importer, Imported, Alias), Imports),
            Rows).

%% check_module_cycles(+Imports, -Diagnostics) is det.
%
% Convert host-supplied module imports to a directed graph and enumerate
% canonical simple cycles. Every cycle repeats its starting module at the end
% so the complete dependency path is visible in one diagnostic.
check_module_cycles(Imports, Diagnostics) :-
    must_be(ground, Imports),
    import_graph(Imports, Graph, DirectedEdges),
    findall(Cycle,
            canonical_module_cycle(Graph, DirectedEdges, Cycle),
            Cycles0),
    sort(Cycles0, Cycles),
    maplist(module_cycle_diagnostic, Cycles, Diagnostics).

import_graph(Imports, Graph, DirectedEdges) :-
    findall(Importer-Imported,
            member(module_import(Importer, Imported, _), Imports),
            DirectedEdges0),
    sort(DirectedEdges0, DirectedEdges),
    findall(Module,
            ( member(Importer-Imported, DirectedEdges),
              ( Module = Importer
              ; Module = Imported
              )
            ),
            Modules0),
    sort(Modules0, Modules),
    vertices_edges_to_ugraph(Modules, DirectedEdges, Graph).

canonical_module_cycle(Graph, DirectedEdges, Canonical) :-
    member(From-To, DirectedEdges),
    simple_path(To, From, Graph, [From], Path),
    normalize_cycle([From | Path], Canonical).

simple_path(Current, Current, _, _, [Current]) :-
    !.
simple_path(Current, Goal, Graph, Visited, [Current | Path]) :-
    neighbors(Current, Graph, Neighbors),
    member(Next, Neighbors),
    (   Next == Goal
    ->  Path = [Goal]
    ;   \+ memberchk(Next, Visited),
        simple_path(Next, Goal, Graph, [Current | Visited], Path)
    ).

normalize_cycle(Cycle, Canonical) :-
    Cycle = [First | _],
    append(Core, [First], Cycle),
    findall(ClosedRotation,
            ( list_rotation(Core, Rotation),
              Rotation = [Start | _],
              append(Rotation, [Start], ClosedRotation)
            ),
            Rotations),
    sort(Rotations, [Canonical | _]).

list_rotation(List, Rotation) :-
    append(Prefix, Suffix, List),
    Suffix \== [],
    append(Suffix, Prefix, Rotation).

module_cycle_diagnostic(Cycle,
                        diagnostic(module, none,
                                   module_dependency_cycle(Cycle))).
