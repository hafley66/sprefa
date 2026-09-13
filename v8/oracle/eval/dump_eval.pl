% Freeze v7 evaluate/4 output for one fixture as JSON.
%
%   swipl dump_eval.pl -- <fixture.pl> <out.json>
%
% The fixture defines program(Rules, Seeds) in v7 checked-goal terms.
% V7_DIR selects the v7 tree (default: ../../../v7 relative to this file).
% Output shape matches v8/src/_6_eval/_6_json.rs.
:- use_module(library(json)).
:- prolog_load_context(directory, Here), atom_concat(Here, '/json_terms', Helpers), consult(Helpers).
:- initialization(main, main).

main([FixtureFile, OutFile]) :-
    dump_fixture(FixtureFile, OutFile).

dump_fixture(FixtureFile, OutFile) :-
    v7_dir(V7),
    atomic_list_concat([V7, '/src/1_libtime/0_evaluator'], EvaluatorPath),
    use_module(EvaluatorPath, [evaluate/4]),
    consult(FixtureFile),
    program(Rules, Seeds),
    evaluate(Rules, Seeds, Closure, Diagnostics),
    maplist(rule_json, Rules, RuleJson),
    maplist(call_json, Seeds, SeedJson),
    maplist(call_json, Closure, ClosureJson),
    maplist(diagnostic_json, Diagnostics, DiagnosticJson),
    Dict = _{ program: _{ rules: RuleJson, seeds: SeedJson },
              expected: _{ closure: ClosureJson, diagnostics: DiagnosticJson } },
    setup_call_cleanup(
        open(OutFile, write, Stream),
        json_write_dict(Stream, Dict, [width(0)]),
        close(Stream)).

v7_dir(V7) :-
    (   getenv('V7_DIR', V7)
    ->  true
    ;   prolog_load_context(directory, Here),
        atomic_list_concat([Here, '/../../../v7'], V7)
    ).
