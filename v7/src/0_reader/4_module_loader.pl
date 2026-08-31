:- module(dl7_module_loader,
          [ load_dl7_units/3,
            load_dl7_project/4
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

%% load_dl7_project(+Root, +Paths, -Project, -Diagnostics) is det.
%
% Canonicalize one filesystem root and retain every source as an independent
% unit. The comptime filesystem grapher later turns their relative paths into
% ordinary product nodes and colon edges.
load_dl7_project(Root, Paths,
                 dl7_project(CanonicalRoot, Units), Diagnostics) :-
    must_be(text, Root),
    once(absolute_file_name(Root, CanonicalRoot,
                            [ file_type(directory),
                              access(read),
                              file_errors(error)
                            ])),
    load_dl7_units(Paths, Units, Diagnostics).
