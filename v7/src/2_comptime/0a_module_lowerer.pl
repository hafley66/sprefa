:- module(dl7_module_lowerer,
          [ lower_units/4,
            merge_module_basements/4
          ]).

:- use_module('0_lowerer', [lower_datalog/4]).

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

unit_module_owner(dl7_unit(Origin, _, _, _, _), module(Origin)).

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
