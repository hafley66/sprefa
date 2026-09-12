% Freeze v7 macrotime (reify, expand, materialize) for one .dl7 fixture.
%
%   swipl v8/oracle/macrotime/dump_macrotime.pl -- <fixture.dl7> <out-prefix>
%
% Run from the sprefa repository root so v7 finds its prelude. Writes
% <out-prefix>_<n>.json, one per distinct reader unit the compile touched.
% The embedder reifies the prelude and macrotime units without expanding them;
% those cases get the same standard macro program the program unit used, so
% every case is a full reify/expand/materialize triple. V7_DIR selects the v7
% tree.
:- use_module(library(json)).
:- prolog_load_context(directory, Here),
   atom_concat(Here, '/../eval/json_terms', Helpers),
   consult(Helpers).
:- initialization(main, main).

:- dynamic seen_reify/1.
:- dynamic reify_case/4.
:- dynamic observed_macro_program/1.
:- dynamic recorded_expand/5.
:- dynamic recorded_materialize/4.

main([Fixture, Prefix]) :-
    v7_dir(V7),
    atomic_list_concat([V7, '/src/2_comptime/2_compiler'], CompilerPath),
    use_module(CompilerPath, [compile_dl7/4]),
    atomic_list_concat([V7, '/src/0_reader/1b_syntax_materializer'], MatPath),
    use_module(MatPath, [materialize_syntax/4]),
    atomic_list_concat([V7, '/src/1_libtime/1_syntax_expander'], ExpPath),
    use_module(ExpPath, [expand_syntax/5]),
    install_wrappers,
    compile_dl7(Fixture, _Rows, _Runtime, CompileDiagnostics),
    format(user_error, "compile diagnostics: ~q~n", [CompileDiagnostics]),
    write_cases(Prefix, 0, Written),
    format(user_error, "cases: ~d~n", [Written]).

install_wrappers :-
    wrap_predicate(dl7_syntax_grapher:reify_syntax(Forms, SourceRows,
                                                   SyntaxRows, ReifyDiags),
                   dump_macrotime_reify, WrapReify,
                   ( WrapReify,
                     record_reify(Forms, SourceRows, SyntaxRows,
                                  ReifyDiags) )),
    wrap_predicate(dl7_syntax_expander:expand_syntax(Rows, MacroProgram,
                                                     Expanded, Origin,
                                                     ExpandDiags),
                   dump_macrotime_expand, WrapExpand,
                   ( WrapExpand,
                     record_expand(Rows, MacroProgram, Expanded, Origin,
                                   ExpandDiags) )),
    wrap_predicate(dl7_syntax_materializer:materialize_syntax(
                       MatRows, MatForms, MatSources, MatDiags),
                   dump_macrotime_materialize, WrapMat,
                   ( WrapMat,
                     assertz(recorded_materialize(MatRows, MatForms,
                                                  MatSources, MatDiags)) )).

v7_dir(V7) :-
    (   getenv('V7_DIR', V7)
    ->  true
    ;   V7 = v7
    ).

record_reify(Forms, SourceRows, SyntaxRows, ReifyDiags) :-
    (   seen_reify(Forms)
    ->  true
    ;   assertz(seen_reify(Forms)),
        assertz(reify_case(Forms, SourceRows, SyntaxRows, ReifyDiags))
    ).

record_expand(Rows, MacroProgram, Expanded, Origin, ExpandDiags) :-
    (   observed_macro_program(_)
    ->  true
    ;   assertz(observed_macro_program(MacroProgram))
    ),
    assertz(recorded_expand(Rows, MacroProgram, Expanded, Origin,
                            ExpandDiags)).

write_cases(Prefix, Index, Written) :-
    (   retract(reify_case(Forms, SourceRows, SyntaxRows, ReifyDiags))
    ->  case_json(Forms, SourceRows, SyntaxRows, ReifyDiags, Dict),
        format(atom(OutFile), "~w_~d.json", [Prefix, Index]),
        setup_call_cleanup(
            open(OutFile, write, Stream),
            json_write_dict(Stream, Dict, [width(0)]),
            close(Stream)),
        format(user_error, "~w~n", [OutFile]),
        Next is Index + 1,
        write_cases(Prefix, Next, Written)
    ;   Written = Index
    ).

case_json(Forms, SourceRows, SyntaxRows, ReifyDiags, Dict) :-
    macro_program(MacroProgram),
    expansion(ReifyDiags, SyntaxRows, MacroProgram,
              Expanded, Origin, ExpandDiags),
    append(ReifyDiags, ExpandDiags, Diagnostics),
    materialization(Diagnostics, Expanded, MatForms, MatSources, MatDiags),
    maplist(term_json, Forms, FormJson),
    maplist(term_json, SourceRows, SourceJson),
    maplist(term_json, SyntaxRows, SyntaxJson),
    maplist(term_json, Expanded, ExpandedJson),
    maplist(term_json, Origin, OriginJson),
    maplist(term_json, Diagnostics, DiagnosticJson),
    maplist(term_json, MatForms, MatFormJson),
    maplist(term_json, MatSources, MatSourceJson),
    maplist(term_json, MatDiags, MatDiagJson),
    macro_program_json(MacroProgram, MacroJson),
    Dict = _{ input: _{ forms: FormJson,
                        source_rows: SourceJson,
                        macro_program: MacroJson },
              expected: _{ syntax_rows: SyntaxJson,
                           expanded_rows: ExpandedJson,
                           origin_rows: OriginJson,
                           diagnostics: DiagnosticJson,
                           forms: MatFormJson,
                           source_rows: MatSourceJson,
                           materialize_diagnostics: MatDiagJson } }.

macro_program(MacroProgram) :-
    (   observed_macro_program(MacroProgram)
    ->  true
    ;   MacroProgram = checked_datalog(root_graph([], []),
                                       datalog_program([], [], []), [], [])
    ).

expansion([], SyntaxRows, MacroProgram, Expanded, Origin, ExpandDiags) :-
    !,
    (   recorded_expand(SyntaxRows, MacroProgram, Expanded0, Origin0,
                        ExpandDiags0)
    ->  Expanded = Expanded0, Origin = Origin0, ExpandDiags = ExpandDiags0
    ;   expand_syntax(SyntaxRows, MacroProgram, Expanded, Origin, ExpandDiags)
    ).
expansion(_, _, _, [], [], []).

materialization([], Expanded, MatForms, MatSources, MatDiags) :-
    !,
    (   recorded_materialize(Expanded, MatForms0, MatSources0, MatDiags0)
    ->  MatForms = MatForms0, MatSources = MatSources0, MatDiags = MatDiags0
    ;   materialize_syntax(Expanded, MatForms, MatSources, MatDiags)
    ).
materialization(_, _, [], [], []).

macro_program_json(
    checked_datalog(root_graph(_, Edges),
                    datalog_program(Relations, Seeds, Rules), _, _),
    _{ edges: EdgeJson, relations: RelationJson,
       seeds: SeedJson, rules: RuleJson }) :-
    !,
    maplist(term_json, Edges, EdgeJson),
    maplist(term_json, Relations, RelationJson),
    maplist(call_json, Seeds, SeedJson),
    maplist(rule_json, Rules, RuleJson).
macro_program_json(_, _{ edges: [], relations: [], seeds: [], rules: [] }).
