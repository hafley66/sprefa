:- begin_tests(dl7_syntax_expander).

:- use_module('../src/0_reader/0_parser', [read_dl7/5]).
:- use_module('../src/0_reader/1a_syntax_grapher', [reify_syntax/4]).
:- use_module('../src/1_libtime/1_syntax_expander', [expand_syntax/5]).
:- use_module('../src/2_comptime/2_compiler', [compile_dl7/4]).

test(dl7_rules_delete_and_splice_nested_syntax_occurrences) :-
    compile_dl7('v7/test/fixtures/14_syntax_macros.dl7',
                _, MacroProgram, CompileDiagnostics),
    Text = "(keep (splice2 alpha beta) omega)\n(drop erased)\n(splice2 (drop later) final)\n",
    read_dl7(macro_source, Text, Forms, SourceRows, ReaderDiagnostics),
    reify_syntax(Forms, SourceRows, Rows, ReifyDiagnostics),
    expand_syntax(Rows, MacroProgram, Expanded, Provenance,
                  ExpansionDiagnostics),
    syntax_snapshot(Expanded, Snapshot),
    provenance_snapshot(Provenance, ProvenanceSnapshot),
    Observed = macro_result(
                   CompileDiagnostics, ReaderDiagnostics,
                   ReifyDiagnostics, ExpansionDiagnostics,
                   Snapshot, ProvenanceSnapshot),
    Observed ==
        macro_result(
            [], [], [], [],
            [ frontier(0, 0),
              frontier(1, 15),
              form(0, [1, 4, 5, 6]),
              atom(1, keep),
              atom(4, alpha),
              atom(5, beta),
              atom(6, omega),
              atom(15, final)
            ],
            [ claim(2, "splice2", 0),
              claim(7, "drop", 0),
              claim(10, "splice2", 0),
              claim(12, "drop", 1),
              output(2, "splice2", 0, 4, 0),
              output(2, "splice2", 0, 5, 1),
              output(10, "splice2", 0, 12, 0),
              output(10, "splice2", 0, 15, 1)
            ]).

syntax_snapshot(Rows, Snapshot) :-
    findall(frontier(Index, Node),
            member(syntax_frontier(Index, reader_node(_, Node)), Rows),
            Frontiers),
    findall(form(Node, Children),
            ( member(syntax_form(reader_node(_, Node)), Rows),
              findall(Index-Child,
                      member(':'(reader_node(_, Node), item,
                                 ref(reader_node(_, Child)), Index), Rows),
                      ChildPairs),
              keysort(ChildPairs, OrderedChildren),
              pair_values(OrderedChildren, Children)
            ),
            Forms),
    findall(atom(Node, Name),
            member(syntax_atom(reader_node(_, Node), Name), Rows),
            Atoms),
    append([Frontiers, Forms, Atoms], Snapshot).

provenance_snapshot(Provenance, Snapshot) :-
    findall(claim(Node, Macro, Wave),
            member(expansion_claim(reader_node(_, Node), Macro, Wave),
                   Provenance),
            Claims),
    findall(output(Node, Macro, Wave, Output, Ordinal),
            member(expansion_output(reader_node(_, Node), Macro, Wave,
                                    reader_node(_, Output), Ordinal),
                   Provenance),
            Outputs),
    append(Claims, Outputs, Snapshot0),
    sort(Snapshot0, Snapshot).

pair_values([], []).
pair_values([_-Value | Pairs], [Value | Values]) :-
    pair_values(Pairs, Values).

:- end_tests(dl7_syntax_expander).
