:- module(dl7_expander,
          [ expand_dl7/6,
            dl7_syntax_rewrite/3
          ]).

:- use_module(library(error), [must_be/2]).

:- multifile dl7_syntax_rewrite/3.

expand_dl7(Forms, SourceRows,
           ExpandedForms, ExpandedSourceRows,
           ExpansionRows, Diagnostics) :-
    must_be(list, Forms),
    must_be(list, SourceRows),
    once(expand_nodes(Forms, SourceRows, Result)),
    expansion_result(Result, SourceRows,
                     ExpandedForms, ExpandedSourceRows,
                     ExpansionRows, Diagnostics).

expansion_result(ok(Forms, GeneratedRows, ExpansionRows), SourceRows,
                 Forms, ExpandedSourceRows, ExpansionRows, []) :-
    append(SourceRows, GeneratedRows, ExpandedSourceRows).
expansion_result(error(Diagnostic), _,
                 [], [], [], [Diagnostic]).

expand_nodes([], _, ok([], [], [])).
expand_nodes([Node | Nodes], AvailableRows, Result) :-
    expand_node(Node, AvailableRows, NodeResult),
    continue_nodes(Nodes, AvailableRows, NodeResult, Result).

continue_nodes(_, _, error(Diagnostic), error(Diagnostic)).
continue_nodes(Nodes, AvailableRows,
               ok(Node, NodeRows, NodeExpansions), Result) :-
    append(AvailableRows, NodeRows, RowsForRest),
    expand_nodes(Nodes, RowsForRest, RestResult),
    (   RestResult = ok(RestNodes, RestRows, RestExpansions)
    ->  append(NodeRows, RestRows, GeneratedRows),
        append(NodeExpansions, RestExpansions, ExpansionRows),
        Result = ok([Node | RestNodes], GeneratedRows, ExpansionRows)
    ;   Result = RestResult
    ).

expand_node(Node0, AvailableRows, Result) :-
    expand_node_children(Node0, AvailableRows, ChildResult),
    (   ChildResult = ok(Node, ChildRows, ChildExpansions)
    ->  append(AvailableRows, ChildRows, RowsForRewrite),
        node_tree(Node, Tree),
        rewrite_fixpoint(Node, Tree, RowsForRewrite, 1, [Tree], [],
                         RewriteResult),
        combine_node_results(ChildRows, ChildExpansions,
                             RewriteResult, Result)
    ;   Result = ChildResult
    ).

expand_node_children(node(NodeId, form(Children)), AvailableRows, Result) :-
    !,
    expand_nodes(Children, AvailableRows, ChildrenResult),
    (   ChildrenResult = ok(Expanded, GeneratedRows, ExpansionRows)
    ->  Result = ok(node(NodeId, form(Expanded)),
                    GeneratedRows, ExpansionRows)
    ;   Result = ChildrenResult
    ).
expand_node_children(Node, _, ok(Node, [], [])).

combine_node_results(_, _, error(Diagnostic), error(Diagnostic)).
combine_node_results(ChildRows, ChildExpansions,
                     ok(Node, RewriteRows, RewriteExpansions),
                     ok(Node, GeneratedRows, ExpansionRows)) :-
    append(ChildRows, RewriteRows, GeneratedRows),
    append(ChildExpansions, RewriteExpansions, ExpansionRows).

rewrite_fixpoint(Node, Tree, AvailableRows, Wave, Seen, MacroTrace, Result) :-
    (   once(dl7_syntax_rewrite(Tree, MacroIdentity, Replacement))
    ->  ensure_rewrite(MacroIdentity, Replacement),
        apply_rewrite(Node, Replacement, MacroIdentity, AvailableRows,
                      Wave, Seen, MacroTrace, Result)
    ;   Result = ok(Node, [], [])
    ).

apply_rewrite(Node, Replacement, MacroIdentity, AvailableRows,
              _, Seen, MacroTrace, error(Diagnostic)) :-
    memberchk(Replacement, Seen),
    !,
    Node = node(NodeId, _),
    reverse([MacroIdentity | MacroTrace], MacroPath),
    expansion_diagnostic(AvailableRows, NodeId,
                         expansion_cycle(MacroPath), Diagnostic).
apply_rewrite(node(InputNodeId, _), Replacement, MacroIdentity, AvailableRows,
              Wave, Seen, MacroTrace, Result) :-
    mint_tree(Replacement, InputNodeId, MacroIdentity, Wave, 0,
              AvailableRows, MintedNode, MintedRows, MintedExpansions, _),
    append(AvailableRows, MintedRows, RowsWithMinted),
    expand_node_children(MintedNode, RowsWithMinted, ChildResult),
    continue_rewrite(ChildResult, MintedRows, MintedExpansions,
                     MacroIdentity, Wave, Seen, MacroTrace,
                     RowsWithMinted, Result).

continue_rewrite(error(Diagnostic), _, _, _, _, _, _, _,
                 error(Diagnostic)).
continue_rewrite(ok(Node, ChildRows, ChildExpansions),
                 MintedRows, MintedExpansions,
                 MacroIdentity, Wave, Seen, MacroTrace,
                 AvailableRows, Result) :-
    append(AvailableRows, ChildRows, RowsForNext),
    node_tree(Node, NextTree),
    NextWave is Wave + 1,
    rewrite_fixpoint(Node, NextTree, RowsForNext, NextWave,
                     [NextTree | Seen], [MacroIdentity | MacroTrace],
                     NextResult),
    combine_rewrite_results(MintedRows, MintedExpansions,
                            ChildRows, ChildExpansions,
                            NextResult, Result).

combine_rewrite_results(_, _, _, _, error(Diagnostic), error(Diagnostic)).
combine_rewrite_results(MintedRows, MintedExpansions,
                        ChildRows, ChildExpansions,
                        ok(Node, NextRows, NextExpansions),
                        ok(Node, GeneratedRows, ExpansionRows)) :-
    append([MintedRows, ChildRows, NextRows], GeneratedRows),
    append([MintedExpansions, ChildExpansions, NextExpansions],
           ExpansionRows).

ensure_rewrite(MacroIdentity, Replacement) :-
    must_be(ground, MacroIdentity),
    must_be(ground, Replacement),
    (   valid_tree(Replacement)
    ->  true
    ;   throw(error(domain_error(dl7_rewrite_tree, Replacement), _))
    ).

valid_tree(atom(Name)) :- atom(Name).
valid_tree(literal(Value)) :- ground(Value).
valid_tree(variable(VariableId, Name)) :-
    ground(VariableId),
    atom(Name).
valid_tree(form(Children)) :-
    is_list(Children),
    maplist(valid_tree, Children).

node_tree(node(_, atom(Name)), atom(Name)).
node_tree(node(_, literal(Value)), literal(Value)).
node_tree(node(_, variable(VariableId, Name)),
          variable(VariableId, Name)).
node_tree(node(_, form(Nodes)), form(Trees)) :-
    maplist(node_tree, Nodes, Trees).

mint_tree(Tree, InputNodeId, MacroIdentity, Wave, Index0, AvailableRows,
          node(NodeId, Payload), GeneratedRows, ExpansionRows, Index) :-
    NodeId = expansion_node(InputNodeId, MacroIdentity, Wave, Index0),
    NextIndex is Index0 + 1,
    source_for_node(AvailableRows, InputNodeId, Source),
    generated_source(Source, NodeId, GeneratedSource),
    mint_payload(Tree, InputNodeId, MacroIdentity, Wave, NextIndex,
                 AvailableRows, Payload, ChildRows, ChildExpansions, Index),
    GeneratedRows = [GeneratedSource | ChildRows],
    ExpansionRows = [ expansion(InputNodeId, MacroIdentity, Wave, NodeId)
                    | ChildExpansions
                    ].

mint_payload(atom(Name), _, _, _, Index, _, atom(Name), [], [], Index).
mint_payload(literal(Value), _, _, _, Index, _,
             literal(Value), [], [], Index).
mint_payload(variable(VariableId, Name), _, _, _, Index, _,
             variable(VariableId, Name), [], [], Index).
mint_payload(form(Trees), InputNodeId, MacroIdentity, Wave, Index0,
             AvailableRows, form(Nodes), GeneratedRows, ExpansionRows, Index) :-
    mint_trees(Trees, InputNodeId, MacroIdentity, Wave, Index0,
               AvailableRows, Nodes, GeneratedRows, ExpansionRows, Index).

mint_trees([], _, _, _, Index, _, [], [], [], Index).
mint_trees([Tree | Trees], InputNodeId, MacroIdentity, Wave, Index0,
           AvailableRows, [Node | Nodes], GeneratedRows, ExpansionRows, Index) :-
    mint_tree(Tree, InputNodeId, MacroIdentity, Wave, Index0, AvailableRows,
              Node, NodeRows, NodeExpansions, NextIndex),
    mint_trees(Trees, InputNodeId, MacroIdentity, Wave, NextIndex,
               AvailableRows, Nodes, RestRows, RestExpansions, Index),
    append(NodeRows, RestRows, GeneratedRows),
    append(NodeExpansions, RestExpansions, ExpansionRows).

source_for_node(Rows, NodeId,
                source(NodeId, Path, StartOffset, EndOffset,
                       StartLine, StartColumn, EndLine, EndColumn)) :-
    memberchk(source(NodeId, Path, StartOffset, EndOffset,
                     StartLine, StartColumn, EndLine, EndColumn), Rows),
    !.
source_for_node(_, NodeId, _) :-
    throw(error(existence_error(source_row, NodeId), _)).

generated_source(source(_, Path, StartOffset, EndOffset,
                        StartLine, StartColumn, EndLine, EndColumn),
                 NodeId,
                 source(NodeId, Path, StartOffset, EndOffset,
                        StartLine, StartColumn, EndLine, EndColumn)).

expansion_diagnostic(Rows, NodeId, Code,
                     diagnostic(expansion, Path, NodeId, Code,
                                position(StartOffset,
                                         StartLine, StartColumn))) :-
    source_for_node(Rows, NodeId,
                    source(NodeId, Path, StartOffset, _,
                           StartLine, StartColumn, _, _)).
