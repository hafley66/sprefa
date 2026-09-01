:- module(dl7_module_lowerer,
          [ lower_units/4,
            lower_units_deferred/4,
            lower_units_with_environment/5,
            lower_units_with_exporter/5,
            lower_units_with_exporter_deferred/5,
            lower_units_with_exporter_and_environment/6,
            merge_module_basements/4,
            install_module_aliases/6
          ]).

:- use_module('0_lowerer',
              [ lower_datalog/4,
                lower_datalog/5,
                lower_datalog_deferred/5
              ]).

%% lower_units(+Units, -ModuleBasements, -ModuleOrigins, -Diagnostics) is det.
%
% Lower each immutable reader unit under its own source-stable module owner.
% The wrapper rows retain the declaring module when basements are later merged.
lower_units([], [], [], []).
lower_units([Unit | Units],
            [module_basement(ModuleOwner, Basement) | ModuleBasements],
            [module_origins(ModuleOwner, Origins) | ModuleOrigins],
            Diagnostics) :-
    unit_module_owner(Unit, ModuleOwner),
    lower_datalog(Unit, Basement, Origins, UnitDiagnostics),
    lower_units(Units, ModuleBasements, ModuleOrigins, RestDiagnostics),
    append(UnitDiagnostics, RestDiagnostics, Diagnostics).

%% lower_units_deferred(+Units, -Basements, -Origins, -Diagnostics) is det.
%
% Bootstrap lowering retains all declarations while holding back executable
% forms that refer to relations generated later by the compiler fixpoint.
lower_units_deferred(Units, Basements, Origins, Diagnostics) :-
    EmptyEnvironment = expression_environment([], [], []),
    lower_units_deferred(Units, EmptyEnvironment,
                         Basements, Origins, Diagnostics).

lower_units_deferred([], _, [], [], []).
lower_units_deferred(
    [Unit | Units], Environment,
    [module_basement(ModuleOwner, Basement) | ModuleBasements],
    [module_origins(ModuleOwner, Origins) | ModuleOrigins], Diagnostics) :-
    unit_module_owner(Unit, ModuleOwner),
    lower_datalog_deferred(Unit, Environment, Basement, Origins,
                           UnitDiagnostics),
    lower_units_deferred(Units, Environment,
                         ModuleBasements, ModuleOrigins, RestDiagnostics),
    append(UnitDiagnostics, RestDiagnostics, Diagnostics).

%% lower_units_with_environment(+Units, +Environment,
%%                              -Basements, -Origins, -Diagnostics) is det.
lower_units_with_environment([], _, [], [], []).
lower_units_with_environment(
    [Unit | Units], Environment,
    [module_basement(ModuleOwner, Basement) | ModuleBasements],
    [module_origins(ModuleOwner, Origins) | ModuleOrigins], Diagnostics) :-
    unit_module_owner(Unit, ModuleOwner),
    lower_datalog(Unit, Environment, Basement, Origins, UnitDiagnostics),
    lower_units_with_environment(Units, Environment,
                                 ModuleBasements, ModuleOrigins,
                                 RestDiagnostics),
    append(UnitDiagnostics, RestDiagnostics, Diagnostics).

unit_module_owner(dl7_unit(Origin, _, _, _, _), module(Origin)).

%% lower_units_with_exporter(+ExporterUnit, +ImporterUnits,
%%                           -ModuleBasements, -ModuleOrigins,
%%                           -Diagnostics) is det.
%
% Lower the exporter first. Its top-level callable declarations become the
% expression environment used while lowering every importer. The resulting
% basements still contain only rows authored by their own source units; alias
% edges are installed after all units have lowered successfully.
lower_units_with_exporter(ExporterUnit, ImporterUnits,
                          ModuleBasements, ModuleOrigins, Diagnostics) :-
    unit_module_owner(ExporterUnit, ExporterOwner),
    lower_datalog(ExporterUnit, ExporterBasement, ExporterOrigins,
                  ExporterDiagnostics),
    lower_importers_after_exporter(
        ExporterDiagnostics, ExporterOwner,
        ExporterBasement, ExporterOrigins, ImporterUnits,
        ModuleBasements, ModuleOrigins, Diagnostics).

%% lower_units_with_exporter_deferred(+Exporter, +Importers,
%%                                    -Basements, -Origins,
%%                                    -Diagnostics) is det.
lower_units_with_exporter_deferred(ExporterUnit, ImporterUnits,
                                   ModuleBasements, ModuleOrigins,
                                   Diagnostics) :-
    unit_module_owner(ExporterUnit, ExporterOwner),
    EmptyEnvironment = expression_environment([], [], []),
    lower_datalog_deferred(ExporterUnit, EmptyEnvironment,
                           ExporterBasement, ExporterOrigins,
                           ExporterDiagnostics),
    lower_importers_after_deferred_exporter(
        ExporterDiagnostics, ExporterOwner,
        ExporterBasement, ExporterOrigins, ImporterUnits,
        ModuleBasements, ModuleOrigins, Diagnostics).

lower_importers_after_deferred_exporter(
    [], ExporterOwner, ExporterBasement, ExporterOrigins, ImporterUnits,
    ModuleBasements, ModuleOrigins, Diagnostics) :-
    !,
    module_expression_environment(ExporterOwner, ExporterBasement,
                                  ImportedEnvironment),
    lower_importing_units_deferred(
        ImporterUnits, ImportedEnvironment,
        ImporterBasements, ImporterOrigins, ImporterDiagnostics),
    ExporterBasements = [module_basement(ExporterOwner, ExporterBasement)
                        | ImporterBasements],
    ExporterModuleOrigins = [module_origins(ExporterOwner, ExporterOrigins)
                            | ImporterOrigins],
    expose_lowered_importers(
        ImporterDiagnostics, ExporterOwner, ImporterUnits,
        ExporterBasements, ExporterModuleOrigins,
        ModuleBasements, ModuleOrigins, Diagnostics).
lower_importers_after_deferred_exporter(
    Diagnostics, ExporterOwner, ExporterBasement, ExporterOrigins, _,
    [module_basement(ExporterOwner, ExporterBasement)],
    [module_origins(ExporterOwner, ExporterOrigins)], Diagnostics).

lower_importing_units_deferred([], _, [], [], []).
lower_importing_units_deferred(
    [Unit | Units],
    expression_environment(Reservations0, Relations, Edges),
    [module_basement(ModuleOwner, Basement) | ModuleBasements],
    [module_origins(ModuleOwner, Origins) | ModuleOrigins], Diagnostics) :-
    unit_module_owner(Unit, ModuleOwner),
    reown_expression_reservations(Reservations0, ModuleOwner, Reservations),
    Environment = expression_environment(Reservations, Relations, Edges),
    lower_datalog_deferred(Unit, Environment, Basement, Origins,
                           UnitDiagnostics),
    lower_importing_units_deferred(
        Units, expression_environment(Reservations0, Relations, Edges),
        ModuleBasements, ModuleOrigins, RestDiagnostics),
    append(UnitDiagnostics, RestDiagnostics, Diagnostics).

%% lower_units_with_exporter_and_environment(+Exporter, +Importers,
%%                                           +GeneratedEnvironment,
%%                                           -Basements, -Origins,
%%                                           -Diagnostics) is det.
%
% Strict final lowering combines ordinary prelude imports with exact-owner
% callable bindings frozen from compiler output.
lower_units_with_exporter_and_environment(
    ExporterUnit, ImporterUnits, GeneratedEnvironment,
    ModuleBasements, ModuleOrigins, Diagnostics) :-
    unit_module_owner(ExporterUnit, ExporterOwner),
    lower_datalog(ExporterUnit, GeneratedEnvironment,
                  ExporterBasement, ExporterOrigins,
                  ExporterDiagnostics),
    lower_importers_after_exporter_environment(
        ExporterDiagnostics, ExporterOwner,
        ExporterBasement, ExporterOrigins, ImporterUnits,
        GeneratedEnvironment,
        ModuleBasements, ModuleOrigins, Diagnostics).

lower_importers_after_exporter_environment(
    [], ExporterOwner, ExporterBasement, ExporterOrigins, ImporterUnits,
    GeneratedEnvironment,
    ModuleBasements, ModuleOrigins, Diagnostics) :-
    !,
    module_expression_environment(ExporterOwner, ExporterBasement,
                                  PreludeEnvironment),
    lower_importing_units_with_environment(
        ImporterUnits, PreludeEnvironment, GeneratedEnvironment,
        ImporterBasements, ImporterOrigins, ImporterDiagnostics),
    ExporterBasements = [module_basement(ExporterOwner, ExporterBasement)
                        | ImporterBasements],
    ExporterModuleOrigins = [module_origins(ExporterOwner, ExporterOrigins)
                            | ImporterOrigins],
    expose_lowered_importers(
        ImporterDiagnostics, ExporterOwner, ImporterUnits,
        ExporterBasements, ExporterModuleOrigins,
        ModuleBasements, ModuleOrigins, Diagnostics).
lower_importers_after_exporter_environment(
    Diagnostics, ExporterOwner, ExporterBasement, ExporterOrigins, _, _,
    [module_basement(ExporterOwner, ExporterBasement)],
    [module_origins(ExporterOwner, ExporterOrigins)], Diagnostics).

lower_importing_units_with_environment([], _, _, [], [], []).
lower_importing_units_with_environment(
    [Unit | Units],
    expression_environment(Reservations0, Relations0, Edges0),
    GeneratedEnvironment,
    [module_basement(ModuleOwner, Basement) | ModuleBasements],
    [module_origins(ModuleOwner, Origins) | ModuleOrigins], Diagnostics) :-
    unit_module_owner(Unit, ModuleOwner),
    reown_expression_reservations(Reservations0, ModuleOwner,
                                  PreludeReservations),
    PreludeOwned = expression_environment(
                       PreludeReservations, Relations0, Edges0),
    merge_expression_environments(GeneratedEnvironment, PreludeOwned,
                                  Environment),
    lower_datalog(Unit, Environment, Basement, Origins, UnitDiagnostics),
    lower_importing_units_with_environment(
        Units, expression_environment(Reservations0, Relations0, Edges0),
        GeneratedEnvironment,
        ModuleBasements, ModuleOrigins, RestDiagnostics),
    append(UnitDiagnostics, RestDiagnostics, Diagnostics).

merge_expression_environments(
    expression_environment(Reservations0, Relations0, Edges0),
    expression_environment(Reservations1, Relations1, Edges1),
    expression_environment(Reservations, Relations, Edges)) :-
    append(Reservations0, Reservations1, Reservations2),
    append(Relations0, Relations1, Relations2),
    append(Edges0, Edges1, Edges2),
    sort(Reservations2, Reservations),
    sort(Relations2, Relations),
    sort(Edges2, Edges).

lower_importers_after_exporter(
    [], ExporterOwner, ExporterBasement, ExporterOrigins, ImporterUnits,
    ModuleBasements, ModuleOrigins, Diagnostics) :-
    !,
    module_expression_environment(ExporterOwner, ExporterBasement,
                                  ImportedEnvironment),
    lower_importing_units(ImporterUnits, ImportedEnvironment,
                          ImporterBasements, ImporterOrigins,
                          ImporterDiagnostics),
    ExporterBasements = [module_basement(ExporterOwner, ExporterBasement)
                        | ImporterBasements],
    ExporterModuleOrigins = [module_origins(ExporterOwner, ExporterOrigins)
                            | ImporterOrigins],
    expose_lowered_importers(
        ImporterDiagnostics, ExporterOwner, ImporterUnits,
        ExporterBasements, ExporterModuleOrigins,
        ModuleBasements, ModuleOrigins, Diagnostics).
lower_importers_after_exporter(
    Diagnostics, ExporterOwner, ExporterBasement, ExporterOrigins, _,
    [module_basement(ExporterOwner, ExporterBasement)],
    [module_origins(ExporterOwner, ExporterOrigins)], Diagnostics).

module_expression_environment(
    ExporterOwner,
    basement_program(root_graph(_, Edges),
                     datalog_program(Relations, _, _)),
    expression_environment(Reservations, Relations, Edges)) :-
    findall(
        reservation(imported, Name, target(Callable), product),
        ( member(pending_edge(ExporterOwner, Name,
                              target(Callable), _), Edges),
          memberchk(relation(Callable, _, _), Relations)
        ),
        Reservations).

lower_importing_units([], _, [], [], []).
lower_importing_units(
    [Unit | Units],
    expression_environment(Reservations0, Relations, Edges),
    [module_basement(ModuleOwner, Basement) | ModuleBasements],
    [module_origins(ModuleOwner, Origins) | ModuleOrigins],
    Diagnostics) :-
    unit_module_owner(Unit, ModuleOwner),
    reown_expression_reservations(Reservations0, ModuleOwner, Reservations),
    Environment = expression_environment(Reservations, Relations, Edges),
    lower_datalog(Unit, Environment, Basement, Origins, UnitDiagnostics),
    lower_importing_units(Units,
                          expression_environment(Reservations0,
                                                 Relations, Edges),
                          ModuleBasements, ModuleOrigins, RestDiagnostics),
    append(UnitDiagnostics, RestDiagnostics, Diagnostics).

reown_expression_reservations([], _, []).
reown_expression_reservations(
    [reservation(_, Name, Target, Kind) | Reservations], Owner,
    [reservation(Owner, Name, Target, Kind) | Reowned]) :-
    reown_expression_reservations(Reservations, Owner, Reowned).

expose_lowered_importers(
    [], ExporterOwner, ImporterUnits, Basements0, Origins0,
    Basements, Origins, []) :-
    !,
    unit_module_owners(ImporterUnits, ImporterOwners),
    install_module_aliases(ExporterOwner, ImporterOwners,
                           Basements0, Origins0, Basements, Origins).
expose_lowered_importers(
    Diagnostics, _, _, Basements, Origins,
    Basements, Origins, Diagnostics).

unit_module_owners([], []).
unit_module_owners([Unit | Units], [Owner | Owners]) :-
    unit_module_owner(Unit, Owner),
    unit_module_owners(Units, Owners).

%% merge_module_basements(+ModuleBasements, +ModuleOrigins,
%%                        -Program, -Origins) is det.
%
% Concatenate already-owned semantic rows in source order. Owner-qualified
% identities keep equal local names from different modules distinct.
merge_module_basements(ModuleBasements, ModuleOrigins,
                       basement_program(root_graph(Nodes, Edges),
                                        datalog_program(Relations, Seeds,
                                                        Rules)),
                       Origins) :-
    merge_basement_rows(ModuleBasements,
                        Nodes, Edges, Relations, Seeds, Rules),
    merge_module_origins(ModuleOrigins, Origins).

merge_basement_rows([], [], [], [], [], []).
merge_basement_rows(
    [module_basement(_, Basement) | ModuleBasements],
    Nodes, Edges, Relations, Seeds, Rules) :-
    Basement = basement_program(
                   root_graph(UnitNodes, UnitEdges),
                   datalog_program(UnitRelations, UnitSeeds, UnitRules)),
    merge_basement_rows(ModuleBasements,
                        RestNodes, RestEdges, RestRelations,
                        RestSeeds, RestRules),
    append(UnitNodes, RestNodes, Nodes),
    append(UnitEdges, RestEdges, Edges),
    append(UnitRelations, RestRelations, Relations),
    append(UnitSeeds, RestSeeds, Seeds),
    append(UnitRules, RestRules, Rules).

merge_module_origins([], []).
merge_module_origins([module_origins(_, UnitOrigins) | ModuleOrigins],
                     Origins) :-
    merge_module_origins(ModuleOrigins, RestOrigins),
    append(UnitOrigins, RestOrigins, Origins).

%% install_module_aliases(+Exporter, +Importers,
%%                        +Basements0, +Origins0,
%%                        -Basements, -Origins) is det.
%
% Expose every concrete top-level exporter edge through each importer. Local
% names shadow these implicit aliases. Alias ordinals follow authored local
% ordinals and retain the exporting edge's source node as provenance.
install_module_aliases(_, [], Basements, Origins, Basements, Origins).
install_module_aliases(Exporter, [Importer | Importers],
                       Basements0, Origins0, Basements, Origins) :-
    module_top_edges(Exporter, Basements0, ExportEdges),
    install_importer_aliases(Importer, Exporter, ExportEdges,
                             Basements0, Origins0,
                             Basements1, Origins1),
    install_module_aliases(Exporter, Importers,
                           Basements1, Origins1, Basements, Origins).

module_top_edges(Owner, Basements, Edges) :-
    memberchk(module_basement(Owner, Basement), Basements),
    Basement = basement_program(root_graph(_, AllEdges), _),
    include(edge_owned_by(Owner), AllEdges, Edges).

edge_owned_by(Owner, pending_edge(Owner, _, _, _)).

install_importer_aliases(Importer, Exporter, ExportEdges,
                         Basements0, Origins0, Basements, Origins) :-
    memberchk(module_basement(Importer, ImporterBasement0), Basements0),
    ImporterBasement0 = basement_program(root_graph(Nodes, Edges0), Program),
    next_owner_index(Importer, Edges0, FirstAliasIndex),
    alias_edges(ExportEdges, Importer, Exporter, Edges0, Origins0,
                FirstAliasIndex, AliasEdges, AliasOrigins),
    append(Edges0, AliasEdges, Edges),
    ImporterBasement = basement_program(root_graph(Nodes, Edges), Program),
    replace_module_basement(Importer, ImporterBasement,
                            Basements0, Basements),
    append_module_origins(Importer, AliasOrigins, Origins0, Origins).

next_owner_index(Owner, Edges, Index) :-
    findall(EdgeIndex,
            member(pending_edge(Owner, _, _, EdgeIndex), Edges),
            Indices),
    (   Indices == []
    ->  Index = 0
    ;   max_list(Indices, Maximum),
        Index is Maximum + 1
    ).

alias_edges([], _, _, _, _, _, [], []).
alias_edges([pending_edge(Exporter, Name, Target, ExportIndex) | ExportEdges],
            Importer, Exporter, ImporterEdges, ModuleOrigins,
            AliasIndex, AliasEdges, AliasOrigins) :-
    (   importable_target(Target),
        \+ memberchk(pending_edge(Importer, Name, _, _), ImporterEdges)
    ->  source_edge_node(ModuleOrigins, Exporter, Name, ExportIndex, NodeId),
        AliasEdges = [pending_edge(Importer, Name, Target, AliasIndex)
                     | RestAliasEdges],
        AliasOrigins = [origin(edge(Importer, Name, AliasIndex), NodeId)
                       | RestAliasOrigins],
        NextAliasIndex is AliasIndex + 1
    ;   AliasEdges = RestAliasEdges,
        AliasOrigins = RestAliasOrigins,
        NextAliasIndex = AliasIndex
    ),
    alias_edges(ExportEdges, Importer, Exporter,
                ImporterEdges, ModuleOrigins, NextAliasIndex,
                RestAliasEdges, RestAliasOrigins).

importable_target(deferred_expression(_)) :- !, fail.
importable_target(deferred_compound_edge(_, _)) :- !, fail.
importable_target(_).

source_edge_node(ModuleOrigins, Module, Name, Index, NodeId) :-
    memberchk(module_origins(Module, Origins), ModuleOrigins),
    memberchk(origin(edge(Module, Name, Index), NodeId), Origins).

replace_module_basement(Module, Replacement,
                        [module_basement(Module, _) | Basements],
                        [module_basement(Module, Replacement) | Basements]) :-
    !.
replace_module_basement(Module, Replacement,
                        [Basement | Basements0],
                        [Basement | Basements]) :-
    replace_module_basement(Module, Replacement, Basements0, Basements).

append_module_origins(Module, Added,
                      [module_origins(Module, Existing) | ModuleOrigins],
                      [module_origins(Module, Combined) | ModuleOrigins]) :-
    !,
    append(Existing, Added, Combined).
append_module_origins(Module, Added,
                      [Origins | ModuleOrigins0],
                      [Origins | ModuleOrigins]) :-
    append_module_origins(Module, Added, ModuleOrigins0, ModuleOrigins).
