:- begin_tests(dl7_syntax_expander).

:- use_module('../src/0_reader/0_parser', [read_dl7/5]).
:- use_module('../src/0_reader/1a_syntax_grapher', [reify_syntax/4]).
:- use_module('../src/0_reader/2_embedder', [dl7_text_unit/5]).
:- use_module('../src/1_libtime/1_syntax_expander', [expand_syntax/5]).
:- use_module('../src/2_comptime/2_compiler',
              [ compile_dl7/4,
                compile_dl7_macro_program/3,
                compile_unit_with_macros/4
              ]).

test(dl7_rules_delete_and_splice_nested_syntax_occurrences) :-
    compile_dl7_macro_program(
        'v7/test/fixtures/14_syntax_macros.dl7',
        MacroProgram, CompileDiagnostics),
    Text = "(keep (splice2 alpha beta) omega)\n(drop erased)\n(splice2 (drop later) final)\n",
    read_dl7(macro_source, Text, Forms, SourceRows, ReaderDiagnostics),
    reify_syntax(Forms, SourceRows, Rows, ReifyDiagnostics),
    expand_syntax(Rows, MacroProgram, Expanded, Provenance,
                  ExpansionDiagnostics),
    syntax_snapshot(Expanded, Snapshot),
    provenance_snapshot(Provenance, ProvenanceSnapshot),
    CompileText = "(splice2 (Alpha: (*)) (Beta: (*)))\n(drop ignored)\n",
    dl7_text_unit(macro_compile, macro_compile_source, CompileText,
                  CompileUnit, CompileReadDiagnostics),
    compile_unit_with_macros(
        CompileUnit, MacroProgram, Compiled, MacroCompileDiagnostics),
    compiled_bind_names(Compiled, BindNames),
    generated_identity_receipt(MacroProgram, GeneratedReceipt),
    PlusText = "(: Action (* (: value int)))\n(: Event (* (: value int)))\n(<+ (Action ?Value) (Event ?Value))\n",
    dl7_text_unit(plus_compile, plus_compile_source, PlusText,
                  PlusUnit, PlusReadDiagnostics),
    compile_unit_with_macros(
        PlusUnit, MacroProgram, PlusCompiled, PlusCompileDiagnostics),
    plus_rule_receipt(PlusCompiled, PlusRuleReceipt),
    Observed = macro_result(
                   CompileDiagnostics, ReaderDiagnostics,
                   ReifyDiagnostics, ExpansionDiagnostics,
                   Snapshot, ProvenanceSnapshot,
                   CompileReadDiagnostics, MacroCompileDiagnostics,
                   BindNames, GeneratedReceipt,
                   PlusReadDiagnostics, PlusCompileDiagnostics,
                   PlusRuleReceipt),
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
            ],
            [], [], ['Alpha', 'Beta'],
            generated_identity(
                arguments([[reader_node(generated_source, 0), 0, 0],
                           [reader_node(generated_source, 2), 0, 0]]),
                constructor_bound(true), stable(true), distinct(true),
                copied_sources(true)),
            [], [], plus_rule(head('Action'), body('Event'),
                             shared_variable(true))).

test(standard_plus_macro_runs_through_normal_file_compilation) :-
    compile_dl7('v7/test/fixtures/15_standard_plus.dl7',
                _, Runtime, Diagnostics),
    absolute_file_name('v7/test/fixtures/15_standard_plus.dl7',
                       CanonicalPath, [access(read)]),
    plus_rule_receipt_for_owner(
        Runtime, module(file(CanonicalPath)), Receipt),
    Diagnostics == [],
    Receipt == plus_rule(head('Action'), body('Event'),
                         shared_variable(true)).

plus_rule_receipt(
    Runtime, Receipt) :-
    plus_rule_receipt_for_owner(Runtime, module(plus_compile), Receipt).

plus_rule_receipt_for_owner(
    compiled_unit(_, Runtime, _), Owner, Receipt) :-
    plus_rule_receipt_for_owner(Runtime, Owner, Receipt).
plus_rule_receipt_for_owner(
    checked_datalog(root_graph(_, Edges),
                    datalog_program(_, _, Rules), _, _),
    Owner,
    plus_rule(head('Action'), body('Event'), shared_variable(Shared))) :-
    memberchk(':'(Owner, 'Action', ref(Action), _), Edges),
    memberchk(':'(Owner, 'Event', ref(Event), _), Edges),
    member(rule(call(ref(Action), [HeadArgument]),
                [checked_goal(positive,
                              call(ref(Event), [BodyArgument]))]),
           Rules),
    equality(HeadArgument, BodyArgument, Shared),
    !.

generated_identity_receipt(MacroProgram, Receipt) :-
    read_dl7(generated_source, "(emit_atom)\n(emit_atom)\n",
             Forms, SourceRows, []),
    reify_syntax(Forms, SourceRows, Rows, []),
    expand_syntax(Rows, MacroProgram, Expanded1, _, []),
    expand_syntax(Rows, MacroProgram, Expanded2, _, []),
    findall(Generated,
            member(syntax_atom(Generated, generated), Expanded1),
            Generated1),
    findall(Generated,
            member(syntax_atom(Generated, generated), Expanded2),
            Generated2),
    maplist(generated_application(Constructor), Generated1, Arguments),
    MacroProgram = checked_datalog(root_graph(_, MacroEdges), _, _, _),
    (   memberchk(':'(_, 'GeneratedSyntax', ref(Constructor), _), MacroEdges)
    ->  ConstructorBound = true
    ;   ConstructorBound = false
    ),
    equality(Generated1, Generated2, Stable),
    distinct_identities(Generated1, Distinct),
    copied_sources(Rows, Expanded1, Generated1, CopiedSources),
    Receipt = generated_identity(
                  arguments(Arguments),
                  constructor_bound(ConstructorBound), stable(Stable),
                  distinct(Distinct), copied_sources(CopiedSources)),
    !.

generated_application(Constructor,
                      application(Constructor, Arguments), Arguments).

equality(Left, Right, true) :- Left == Right, !.
equality(_, _, false).

distinct_identities([Left, Right], true) :- Left \== Right, !.
distinct_identities(_, false).

copied_sources(InputRows, ExpandedRows, Generated, true) :-
    findall(source_position(StartOffset, EndOffset,
                            StartLine, StartColumn, EndLine, EndColumn),
            ( member(syntax_frontier(_, Invocation), InputRows),
              member(source(Invocation, generated_source,
                            StartOffset, EndOffset, StartLine, StartColumn,
                            EndLine, EndColumn), InputRows)
            ),
            InputSources0),
    sort(InputSources0, InputSources),
    findall(source_position(StartOffset, EndOffset,
                            StartLine, StartColumn, EndLine, EndColumn),
            ( member(Identity, Generated),
              member(source(Identity, generated_source,
                            StartOffset, EndOffset, StartLine, StartColumn,
                            EndLine, EndColumn), ExpandedRows)
            ),
            GeneratedSources0),
    sort(GeneratedSources0, GeneratedSources),
    GeneratedSources == InputSources,
    !.
copied_sources(_, _, _, false).

compiled_bind_names(
    compiled_unit(_, checked_datalog(root_graph(_, Edges), _, _, _), _),
    Names) :-
    findall(Name,
            ( member(':'(module(macro_compile), Name, ref(_), _), Edges),
              memberchk(Name, ['Alpha', 'Beta'])
            ),
            Names0),
    sort(Names0, Names).

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
