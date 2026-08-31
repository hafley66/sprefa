:- module(dl7_filesystem_grapher,
          [ install_project_graph/6
          ]).

:- use_module(library(error), [must_be/2]).
:- use_module(library(filesex),
              [ directory_file_path/3,
                relative_file_name/3
              ]).

%% install_project_graph(+Project, +Basements0, +Origins0,
%%                       -Basements, -Origins, -Diagnostics) is det.
%
% Represent the project root, directories, and source modules as products.
% Parent-to-child filesystem names are ordinary pending colon edges, so the
% existing lexical resolver and checked :/4 relation own traversal.
install_project_graph(Project, Basements0, Origins0,
                      Basements, Origins, Diagnostics) :-
    must_be(ground, Project),
    Project = dl7_project(CanonicalRoot, Units),
    project_graph(CanonicalRoot, Units, Nodes, Edges,
                  EdgeOrigins, Diagnostics),
    finish_project_graph(
        Diagnostics, CanonicalRoot, Nodes, Edges, EdgeOrigins,
        Basements0, Origins0, Basements, Origins).

finish_project_graph([], CanonicalRoot, Nodes, Edges, EdgeOrigins,
                     Basements0, Origins0,
                     [module_basement(ProjectOwner, ProjectBasement)
                     | Basements0],
                     [module_origins(ProjectOwner, EdgeOrigins)
                     | Origins0]) :-
    !,
    directory_owner(CanonicalRoot, ProjectOwner),
    ProjectBasement = basement_program(
                          root_graph(Nodes, Edges),
                          datalog_program([], [], [])).
finish_project_graph(_Diagnostics, _, _, _, _,
                     Basements, Origins, Basements, Origins).

project_graph(CanonicalRoot, Units, Nodes, Edges, Origins, Diagnostics) :-
    unit_path_claims(Units, CanonicalRoot, Claims0,
                     DirectoryOwners0, FileOwners0, Diagnostics),
    (   Diagnostics == []
    ->  sort(Claims0, SortedClaims),
        unique_claims(SortedClaims, Claims),
        indexed_claims(Claims, Edges, Origins),
        directory_owner(CanonicalRoot, RootOwner),
        sort([RootOwner | DirectoryOwners0], DirectoryOwners),
        sort(FileOwners0, FileOwners),
        filesystem_nodes(DirectoryOwners, FileOwners, Nodes)
    ;   Nodes = [],
        Edges = [],
        Origins = []
    ).

unit_path_claims([], _, [], [], [], []).
unit_path_claims([Unit | Units], CanonicalRoot,
                 Claims, DirectoryOwners, FileOwners, Diagnostics) :-
    unit_path_claims(Units, CanonicalRoot,
                     RestClaims, RestDirectoryOwners, RestFileOwners,
                     RestDiagnostics),
    unit_path_claim(Unit, CanonicalRoot, UnitResult),
    combine_unit_path_result(
        UnitResult,
        RestClaims, RestDirectoryOwners, RestFileOwners, RestDiagnostics,
        Claims, DirectoryOwners, FileOwners, Diagnostics).

unit_path_claim(dl7_unit(file(CanonicalPath), _, _, _, _),
                CanonicalRoot, Result) :-
    !,
    relative_file_name(CanonicalPath, CanonicalRoot, RelativePath),
    (   outside_project_root(RelativePath)
    ->  Result = error(
                     diagnostic(module, filesystem(CanonicalPath),
                                outside_project_root(
                                    CanonicalRoot, CanonicalPath)))
    ;   module_path_segments(RelativePath, Segments)
    ->  directory_owner(CanonicalRoot, RootOwner),
        file_owner(CanonicalPath, FileOwner),
        path_claims(Segments, CanonicalRoot, [], RootOwner,
                    FileOwner, CanonicalPath,
                    Claims, DirectoryOwners),
        Result = ok(Claims, DirectoryOwners, FileOwner)
    ;   Result = error(
                     diagnostic(module, filesystem(CanonicalPath),
                                invalid_dl7_module_path(RelativePath)))
    ).
unit_path_claim(dl7_unit(Origin, _, _, _, _), _,
                error(diagnostic(module, Origin,
                                 project_unit_without_file_origin))) :-
    !.
unit_path_claim(Unit, _,
                error(diagnostic(module, none,
                                 invalid_project_unit(Unit)))).

combine_unit_path_result(
    ok(UnitClaims, UnitDirectoryOwners, FileOwner),
    RestClaims, RestDirectoryOwners, RestFileOwners, RestDiagnostics,
    Claims, DirectoryOwners, [FileOwner | RestFileOwners], RestDiagnostics) :-
    !,
    append(UnitClaims, RestClaims, Claims),
    append(UnitDirectoryOwners, RestDirectoryOwners, DirectoryOwners).
combine_unit_path_result(
    error(Diagnostic),
    RestClaims, RestDirectoryOwners, RestFileOwners, RestDiagnostics,
    RestClaims, RestDirectoryOwners, RestFileOwners,
    [Diagnostic | RestDiagnostics]).

outside_project_root('..').
outside_project_root(RelativePath) :-
    atom_concat('../', _, RelativePath).

module_path_segments(RelativePath, Segments) :-
    atomic_list_concat(Parts, '/', RelativePath),
    append(DirectoryParts, [FileName], Parts),
    file_name_extension(FileStem, dl7, FileName),
    FileStem \== '',
    append(DirectoryParts, [FileStem], RawSegments),
    maplist(semantic_segment, RawSegments, Segments).

semantic_segment(Raw, Semantic) :-
    atom_codes(Raw, Codes),
    (   append(OrderCodes, [0'_ | NameCodes], Codes),
        OrderCodes \== [],
        NameCodes \== [],
        maplist(decimal_code, OrderCodes)
    ->  atom_codes(Semantic, NameCodes)
    ;   Semantic = Raw
    ).

decimal_code(Code) :-
    Code >= 0'0,
    Code =< 0'9.

path_claims([Label], _, _, ParentOwner, FileOwner, SourcePath,
            [filesystem_claim(ParentOwner, Label, FileOwner, SourcePath)],
            []) :-
    !.
path_claims([Label | Segments], CanonicalRoot, ActualPrefix,
            ParentOwner, FileOwner, SourcePath,
            [filesystem_claim(ParentOwner, Label, DirectoryOwner, SourcePath)
            | Claims],
            [DirectoryOwner | DirectoryOwners]) :-
    segment_actual_name(Label, Segments, SourcePath, ActualName),
    append(ActualPrefix, [ActualName], ChildActualPrefix),
    directory_path(CanonicalRoot, ChildActualPrefix, DirectoryPath),
    directory_owner(DirectoryPath, DirectoryOwner),
    path_claims(Segments, CanonicalRoot, ChildActualPrefix,
                DirectoryOwner, FileOwner, SourcePath,
                Claims, DirectoryOwners).

% The source path determines the exact author-prefixed directory spelling.
segment_actual_name(_, RemainingSegments, SourcePath, ActualName) :-
    length(RemainingSegments, RemainingCount),
    file_directory_name(SourcePath, SourceDirectory),
    directory_ancestor_name(SourceDirectory, RemainingCount, ActualName).

directory_ancestor_name(Directory, 1, Name) :-
    !,
    file_base_name(Directory, Name).
directory_ancestor_name(Directory, RemainingCount, Name) :-
    RemainingCount > 1,
    file_directory_name(Directory, Parent),
    NextCount is RemainingCount - 1,
    directory_ancestor_name(Parent, NextCount, Name).

directory_path(Root, [], Root).
directory_path(Root, [Segment | Segments], Path) :-
    directory_file_path(Root, Segment, Child),
    directory_path(Child, Segments, Path).

directory_owner(Path, module(directory(Path))).
file_owner(Path, module(file(Path))).

unique_claims([], []).
unique_claims([Claim | Claims], [Claim | Unique]) :-
    Claim = filesystem_claim(Owner, Label, Target, _),
    drop_same_claim(Claims, Owner, Label, Target, Rest),
    unique_claims(Rest, Unique).

drop_same_claim(
    [filesystem_claim(Owner, Label, Target, _) | Claims],
    Owner, Label, Target, Rest) :-
    !,
    drop_same_claim(Claims, Owner, Label, Target, Rest).
drop_same_claim(Claims, _, _, _, Claims).

indexed_claims(Claims, Edges, Origins) :-
    indexed_claims(Claims, none, 0, Edges, Origins).

indexed_claims([], _, _, [], []).
indexed_claims(
    [filesystem_claim(Owner, Label, Target, SourcePath) | Claims],
    PreviousOwner, PreviousIndex,
    [pending_edge(Owner, Label, target(Target), Index) | Edges],
    [origin(edge(Owner, Label, Index), filesystem(SourcePath)) | Origins]) :-
    next_owner_index(Owner, PreviousOwner, PreviousIndex, Index),
    indexed_claims(Claims, Owner, Index, Edges, Origins).

next_owner_index(Owner, Owner, PreviousIndex, Index) :-
    !,
    Index is PreviousIndex + 1.
next_owner_index(_, _, _, 0).

filesystem_nodes(DirectoryOwners, FileOwners, Nodes) :-
    directory_nodes(DirectoryOwners, DirectoryNodes),
    file_product_nodes(FileOwners, FileNodes),
    append(DirectoryNodes, FileNodes, Nodes).

directory_nodes([], []).
directory_nodes([Owner | Owners],
                [node(Owner), module(Owner), product(Owner) | Nodes]) :-
    directory_nodes(Owners, Nodes).

file_product_nodes([], []).
file_product_nodes([Owner | Owners], [product(Owner) | Nodes]) :-
    file_product_nodes(Owners, Nodes).
