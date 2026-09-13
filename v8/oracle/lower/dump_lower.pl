% Freeze every lowering the v7 compiler performs for one .dl7 fixture.
%
%   swipl v8/oracle/lower/dump_lower.pl -- <fixture.dl7> <out-prefix>
%
% Run from the sprefa repository root so v7 finds its prelude. V7_DIR selects
% the v7 tree (default: v7). Writes <out-prefix>_<n>.json per lowering.
%
% Both exported entry points are wrapped. lower_datalog/5 carries the strict
% policy and fires only for the macrotime unit; lower_datalog_deferred/5
% carries defer_unknown_calls and is the one the compiler uses for the prelude
% and for every source unit.
%
% v7 0_lowerer.pl:292 leaves Program unbound on the executable-error path
% while every sibling error clause binds []. An unbound Program is written as
% JSON null and counted in the dump summary.
:- use_module(library(json)).
:- prolog_load_context(directory, Here),
   atom_concat(Here, '/../eval/json_terms', Helpers),
   consult(Helpers).
:- initialization(main, main).

:- dynamic call_index/1.
call_index(0).

main([Fixture, Prefix]) :-
    v7_dir(V7),
    atomic_list_concat([V7, '/src/2_comptime/2_compiler'], CompilerPath),
    use_module(CompilerPath, [compile_dl7/4]),
    wrap_predicate(dl7_lowerer:lower_datalog(StrictUnit, StrictEnvironment,
                                             StrictProgram, StrictOrigins,
                                             StrictDiagnostics),
                   dump_lower_strict, Strict,
                   ( Strict,
                     record_call(Prefix, strict, StrictUnit,
                                 StrictEnvironment, StrictProgram,
                                 StrictOrigins, StrictDiagnostics) )),
    wrap_predicate(dl7_lowerer:lower_datalog_deferred(
                       DeferredUnit, DeferredEnvironment, DeferredProgram,
                       DeferredOrigins, DeferredDiagnostics),
                   dump_lower_deferred, Deferred,
                   ( Deferred,
                     record_call(Prefix, defer_unknown_calls, DeferredUnit,
                                 DeferredEnvironment, DeferredProgram,
                                 DeferredOrigins, DeferredDiagnostics) )),
    catch(compile_dl7(Fixture, _Rows, _Runtime, CompileDiagnostics),
          Error,
          ( CompileDiagnostics = [caught(Error)] )),
    format(user_error, "compile diagnostics: ~q~n", [CompileDiagnostics]),
    call_index(N),
    format(user_error, "lowerings: ~d~n", [N]).

v7_dir(V7) :-
    (   getenv('V7_DIR', V7)
    ->  true
    ;   V7 = v7
    ).

record_call(Prefix, Policy, Unit, Environment, Program, Origins,
            Diagnostics) :-
    retract(call_index(N0)),
    N is N0 + 1,
    assertz(call_index(N)),
    term_json(Unit, UnitJson),
    term_json(Environment, EnvironmentJson),
    program_json(Program, ProgramJson, Bound),
    term_json(Origins, OriginsJson),
    term_json(Diagnostics, DiagnosticsJson),
    unit_origin(Unit, UnitOrigin),
    term_json(UnitOrigin, UnitOriginJson),
    Dict = _{ input: _{ policy: Policy, origin: UnitOriginJson,
                        unit: UnitJson, environment: EnvironmentJson },
              expected: _{ program: ProgramJson, program_bound: Bound,
                           origins: OriginsJson,
                           diagnostics: DiagnosticsJson } },
    format(atom(OutFile), "~w_~d.json", [Prefix, N0]),
    setup_call_cleanup(
        open(OutFile, write, Stream),
        json_write_dict(Stream, Dict, [width(0)]),
        close(Stream)),
    format(user_error, "~w policy=~w bound=~w~n", [OutFile, Policy, Bound]).

program_json(Program, null, false) :-
    var(Program),
    !.
program_json(Program, Json, true) :-
    term_json(Program, Json).

unit_origin(dl7_unit(Origin, _, _, _, _), Origin) :- !.
unit_origin(_, unknown).
