% Freeze compile_dl7_macro_program/3 for one macro library, for reference.
%
%   swipl v8/oracle/macrotime/dump_macro_program.pl -- <library.dl7> <out.json>
%
% The cases the test reads carry the SLICED program expand_syntax/5 was given;
% this is the whole checked program that slice comes from. V7_DIR selects the
% v7 tree.
:- use_module(library(json)).
:- prolog_load_context(directory, Here),
   atom_concat(Here, '/../eval/json_terms', Helpers),
   consult(Helpers).
:- initialization(main, main).

main([Path, OutFile]) :-
    v7_dir(V7),
    atomic_list_concat([V7, '/src/2_comptime/2_compiler'], CompilerPath),
    use_module(CompilerPath, [compile_dl7_macro_program/3]),
    atomic_list_concat([V7, '/src/1_libtime/0a_syntax_macro_program'], SlicePath),
    use_module(SlicePath, [slice_macro_program/3]),
    compile_dl7_macro_program(Path, MacroProgram, Diagnostics),
    format(user_error, "diagnostics: ~q~n", [Diagnostics]),
    slice_macro_program(MacroProgram, Sliced, SliceDiagnostics),
    format(user_error, "slice diagnostics: ~q~n", [SliceDiagnostics]),
    program_json(MacroProgram, WholeJson),
    program_json(Sliced, SlicedJson),
    Dict = _{ whole: WholeJson, sliced: SlicedJson },
    setup_call_cleanup(
        open(OutFile, write, Stream),
        json_write_dict(Stream, Dict, [width(0)]),
        close(Stream)).

v7_dir(V7) :-
    (   getenv('V7_DIR', V7)
    ->  true
    ;   V7 = v7
    ).

program_json(
    checked_datalog(root_graph(_, Edges),
                    datalog_program(Relations, Seeds, Rules), _, _),
    _{ edges: EdgeJson, relations: RelationJson,
       seeds: SeedJson, rules: RuleJson }) :-
    !,
    maplist(term_json, Edges, EdgeJson),
    maplist(term_json, Relations, RelationJson),
    maplist(call_json, Seeds, SeedJson),
    maplist(rule_json, Rules, RuleJson).
program_json(_, _{ edges: [], relations: [], seeds: [], rules: [] }).
