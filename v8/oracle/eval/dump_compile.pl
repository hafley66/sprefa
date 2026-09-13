% Freeze every evaluate/4 call the v7 compiler makes for one .dl7 fixture.
%
%   swipl dump_compile.pl -- <fixture.dl7> <out-prefix>
%
% Run from the sprefa repository root so v7 finds its prelude. Writes
% <out-prefix>_<n>.json per evaluate/4 call (macrotime and comptime), same
% shape as dump_eval.pl. V7_DIR selects the v7 tree (default: v7).
:- use_module(library(json)).
:- prolog_load_context(directory, Here), atom_concat(Here, '/json_terms', Helpers), consult(Helpers).
:- initialization(main, main).

:- dynamic call_index/1.
call_index(0).

main([Fixture, Prefix]) :-
    v7_dir(V7),
    atomic_list_concat([V7, '/src/2_comptime/2_compiler'], CompilerPath),
    use_module(CompilerPath, [compile_dl7/4]),
    wrap_predicate(dl7_evaluator:evaluate(Rules, Seeds, Closure, Diagnostics),
                   dump_compile_wrap, Wrapped,
                   ( Wrapped,
                     record_call(Prefix, Rules, Seeds, Closure, Diagnostics) )),
    compile_dl7(Fixture, _Rows, _Runtime, CompileDiagnostics),
    format(user_error, "compile diagnostics: ~q~n", [CompileDiagnostics]),
    call_index(N),
    format(user_error, "evaluate calls: ~d~n", [N]).

v7_dir(V7) :-
    (   getenv('V7_DIR', V7)
    ->  true
    ;   V7 = v7
    ).

record_call(Prefix, Rules, Seeds, Closure, Diagnostics) :-
    retract(call_index(N0)),
    N is N0 + 1,
    assertz(call_index(N)),
    maplist(rule_json, Rules, RuleJson),
    maplist(call_json, Seeds, SeedJson),
    maplist(call_json, Closure, ClosureJson),
    maplist(diagnostic_json, Diagnostics, DiagnosticJson),
    Dict = _{ program: _{ rules: RuleJson, seeds: SeedJson },
              expected: _{ closure: ClosureJson, diagnostics: DiagnosticJson } },
    format(atom(OutFile), "~w_~d.json", [Prefix, N0]),
    setup_call_cleanup(
        open(OutFile, write, Stream),
        json_write_dict(Stream, Dict, [width(0)]),
        close(Stream)),
    length(Rules, RC), length(Seeds, SC), length(Closure, CC),
    format(user_error, "~w: ~d rules ~d seeds ~d closure rows~n", [OutFile, RC, SC, CC]).
