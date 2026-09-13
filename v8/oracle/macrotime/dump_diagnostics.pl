% Freeze v7 macrotime diagnostics. Every .dl7 that compiles reaches macrotime
% clean, so these cases mutate one real unit's reader input or its macro
% program and record what v7 then says.
%
%   swipl v8/oracle/macrotime/dump_diagnostics.pl -- <fixture.dl7> <out-dir>
%
% Run from the sprefa repository root. V7_DIR selects the v7 tree.
:- use_module(library(json)).
:- prolog_load_context(directory, Here),
   atom_concat(Here, '/../eval/json_terms', Helpers),
   consult(Helpers).
:- initialization(main, main).

:- dynamic unit/2.
:- dynamic observed_macro_program/1.

main([Fixture, OutDir]) :-
    v7_dir(V7),
    atomic_list_concat([V7, '/src/2_comptime/2_compiler'], CompilerPath),
    use_module(CompilerPath, [compile_dl7/4]),
    atomic_list_concat([V7, '/src/0_reader/1a_syntax_grapher'], ReifyPath),
    use_module(ReifyPath, [reify_syntax/4]),
    atomic_list_concat([V7, '/src/0_reader/1b_syntax_materializer'], MatPath),
    use_module(MatPath, [materialize_syntax/4]),
    atomic_list_concat([V7, '/src/1_libtime/1_syntax_expander'], ExpPath),
    use_module(ExpPath, [expand_syntax/5]),
    wrap_predicate(dl7_syntax_expander:expand_syntax(Rows, MacroProgram,
                                                     _, _, _),
                   dump_diagnostics_expand, Wrapped,
                   ( Wrapped,
                     record_unit(Rows, MacroProgram) )),
    compile_dl7(Fixture, _, _, _),
    forall(mutation(Name, _), write_mutation(OutDir, Name)).

v7_dir(V7) :-
    (   getenv('V7_DIR', V7)
    ->  true
    ;   V7 = v7
    ).

% The rows the compiler handed the expander are the program unit's; recover its
% reader input from them so a mutation starts from real source.
record_unit(Rows, MacroProgram) :-
    (   unit(_, _)
    ->  true
    ;   materialize_syntax(Rows, Forms, SourceRows, []),
        assertz(unit(Forms, SourceRows)),
        assertz(observed_macro_program(MacroProgram))
    ).

mutation(missing_source, drop_first_source).
mutation(duplicate_source, duplicate_first_source).
mutation(invalid_reader_payload, replace_first_payload).
mutation(invalid_reader_node, replace_first_form).
mutation(missing_protocol_relation, drop_claim_edges).
mutation(reused_syntax_occurrence, repeat_first_child).
mutation(clean, keep_input).

apply_mutation(drop_first_source, Forms, [_ | Sources], Program,
               Forms, Sources, Program).
apply_mutation(duplicate_first_source, Forms, [Source | Sources], Program,
               Forms, [Source, Source | Sources], Program).
apply_mutation(replace_first_payload, [node(Id, _) | Forms], Sources, Program,
               [node(Id, junk) | Forms], Sources, Program).
apply_mutation(replace_first_form, [_ | Forms], Sources, Program,
               [oops | Forms], Sources, Program).
apply_mutation(drop_claim_edges, Forms, Sources,
               checked_datalog(root_graph(Root, Edges),
                               datalog_program(R, S, U), A, B),
               Forms, Sources,
               checked_datalog(root_graph(Root, KeptEdges),
                               datalog_program(R, S, U), A, B)) :-
    exclude(claim_edge, Edges, KeptEdges).
apply_mutation(repeat_first_child,
               [node(Id, form([Child, _ | Rest])) | Forms], Sources, Program,
               [node(Id, form([Child, Child | Rest])) | Forms], Sources,
               Program).
apply_mutation(keep_input, Forms, Sources, Program, Forms, Sources, Program).

claim_edge(':'(_, syntax_claim, _, _)).

write_mutation(OutDir, Name) :-
    unit(Forms0, Sources0),
    observed_macro_program(Program0),
    mutation(Name, Kind),
    apply_mutation(Kind, Forms0, Sources0, Program0, Forms, Sources, Program),
    reify_syntax(Forms, Sources, SyntaxRows, ReifyDiags),
    (   ReifyDiags == []
    ->  expand_syntax(SyntaxRows, Program, Expanded, Origin, ExpandDiags)
    ;   Expanded = [], Origin = [], ExpandDiags = []
    ),
    append(ReifyDiags, ExpandDiags, Diagnostics),
    (   Diagnostics == []
    ->  materialize_syntax(Expanded, MatForms, MatSources, MatDiags)
    ;   MatForms = [], MatSources = [], MatDiags = []
    ),
    maplist(term_json, Forms, FormJson),
    maplist(term_json, Sources, SourceJson),
    maplist(term_json, SyntaxRows, SyntaxJson),
    maplist(term_json, Expanded, ExpandedJson),
    maplist(term_json, Origin, OriginJson),
    maplist(term_json, Diagnostics, DiagnosticJson),
    maplist(term_json, MatForms, MatFormJson),
    maplist(term_json, MatSources, MatSourceJson),
    maplist(term_json, MatDiags, MatDiagJson),
    macro_program_json(Program, MacroJson),
    Dict = _{ input: _{ forms: FormJson,
                        source_rows: SourceJson,
                        macro_program: MacroJson },
              expected: _{ syntax_rows: SyntaxJson,
                           expanded_rows: ExpandedJson,
                           origin_rows: OriginJson,
                           diagnostics: DiagnosticJson,
                           forms: MatFormJson,
                           source_rows: MatSourceJson,
                           materialize_diagnostics: MatDiagJson } },
    format(atom(OutFile), "~w/diagnostic_~w_0.json", [OutDir, Name]),
    setup_call_cleanup(
        open(OutFile, write, Stream),
        json_write_dict(Stream, Dict, [width(0)]),
        close(Stream)),
    length(Diagnostics, DC), length(MatDiags, MC),
    format(user_error, "~w: ~d expand ~d materialize~n", [Name, DC, MC]).

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
