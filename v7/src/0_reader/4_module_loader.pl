:- module(dl7_module_loader,
          [ load_dl7_units/3
          ]).

:- use_module(library(error), [must_be/2]).
:- use_module('3_file_loader', [load_dl7/3]).

%% load_dl7_units(+Paths, -Units, -Diagnostics) is det.
%
% Read every source as a separate immutable unit. Source diagnostics are
% retained in path order; one invalid unit does not splice its forms into a
% neighboring module.
load_dl7_units(Paths, Units, Diagnostics) :-
    must_be(list, Paths),
    load_dl7_units_(Paths, Units, Diagnostics).

load_dl7_units_([], [], []).
load_dl7_units_([Path | Paths], [Unit | Units], Diagnostics) :-
    load_dl7(Path, Unit, PathDiagnostics),
    load_dl7_units_(Paths, Units, RestDiagnostics),
    append(PathDiagnostics, RestDiagnostics, Diagnostics).
