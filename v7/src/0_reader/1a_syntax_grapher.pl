:- module(dl7_syntax_grapher, [reify_syntax/4]).

:- use_module(library(error), [must_be/2]).

%% reify_syntax(+ReaderForms, +SourceRows,
%%              -SyntaxGraphRows, -Diagnostics) is det.
%
% Reify the reader tree without changing any reader or logical-variable
% identity. Form children become ordinary indexed :/4 edges. The frontier is
% a relation because this layer has no module owner yet; its ordinal preserves
% the order of top-level forms.
reify_syntax(ReaderForms, SourceRows, SyntaxGraphRows, Diagnostics) :-
    must_be(list, ReaderForms),
    must_be(list, SourceRows),
    once(reify_frontier(ReaderForms, SourceRows, 0, Result)),
    syntax_graph_result(Result, SyntaxGraphRows, Diagnostics).

syntax_graph_result(ok(Rows0), Rows, []) :-
    sort(Rows0, Rows).
syntax_graph_result(error(Diagnostic), [], [Diagnostic]).

reify_frontier([], _, _, ok([])).
reify_frontier([Node | Nodes], SourceRows, Index, Result) :-
    reify_node(Node, SourceRows, NodeResult),
    continue_frontier(NodeResult, Node, Nodes, SourceRows, Index, Result).

continue_frontier(error(Diagnostic), _, _, _, _, error(Diagnostic)).
continue_frontier(ok(NodeRows), node(NodeId, _), Nodes, SourceRows, Index,
                  Result) :-
    NextIndex is Index + 1,
    reify_frontier(Nodes, SourceRows, NextIndex, RestResult),
    (   RestResult = ok(RestRows)
    ->  append([syntax_frontier(Index, NodeId) | NodeRows], RestRows, Rows),
        Result = ok(Rows)
    ;   Result = RestResult
    ).

reify_node(node(NodeId, Payload), SourceRows, Result) :-
    !,
    source_row_result(SourceRows, NodeId, SourceResult),
    reify_node_after_source(SourceResult, NodeId, Payload, SourceRows, Result).
reify_node(Other, _,
           error(diagnostic(syntax_graph, none, invalid_reader_node(Other)))).

reify_node_after_source(error(Diagnostic), _, _, _, error(Diagnostic)).
reify_node_after_source(ok(Source), NodeId, atom(Name), _, Result) :-
    !,
    (   atom(Name)
    ->  Result = ok([node(NodeId), syntax_atom(NodeId, Name), Source])
    ;   Result = error(diagnostic(
                           syntax_graph, NodeId,
                           invalid_atom_payload(Name)))
    ).
reify_node_after_source(ok(Source), NodeId, literal(Value), _,
                        ok([node(NodeId), syntax_literal(NodeId, Value),
                            Source])) :-
    ground(Value),
    !.
reify_node_after_source(ok(_), NodeId, literal(Value), _,
                        error(diagnostic(
                                  syntax_graph, NodeId,
                                  nonground_literal_payload(Value)))) :-
    !.
reify_node_after_source(ok(Source), NodeId,
                        variable(VariableId, Name), _, Result) :-
    !,
    (   ground(VariableId),
        atom(Name)
    ->  Result = ok([ node(NodeId),
                       syntax_variable(NodeId, VariableId, Name),
                       Source
                     ])
    ;   Result = error(diagnostic(
                           syntax_graph, NodeId,
                           invalid_variable_payload(VariableId, Name)))
    ).
reify_node_after_source(ok(Source), NodeId, form(Children), SourceRows,
                        Result) :-
    !,
    (   is_list(Children)
    ->  reify_children(Children, NodeId, SourceRows, 0, ChildrenResult),
        prepend_form_rows(ChildrenResult, NodeId, Source, Result)
    ;   Result = error(diagnostic(
                           syntax_graph, NodeId,
                           invalid_form_children(Children)))
    ).
reify_node_after_source(ok(_), NodeId, Payload, _,
                        error(diagnostic(
                                  syntax_graph, NodeId,
                                  invalid_reader_payload(Payload)))).

prepend_form_rows(error(Diagnostic), _, _, error(Diagnostic)).
prepend_form_rows(ok(ChildRows), NodeId, Source,
                  ok([node(NodeId), syntax_form(NodeId), Source | ChildRows])).

reify_children([], _, _, _, ok([])).
reify_children([Child | Children], Owner, SourceRows, Index, Result) :-
    reify_node(Child, SourceRows, ChildResult),
    continue_children(ChildResult, Child, Children, Owner, SourceRows, Index,
                      Result).

continue_children(error(Diagnostic), _, _, _, _, _, error(Diagnostic)).
continue_children(ok(ChildRows), node(ChildId, _), Children, Owner,
                  SourceRows, Index, Result) :-
    NextIndex is Index + 1,
    reify_children(Children, Owner, SourceRows, NextIndex, RestResult),
    (   RestResult = ok(RestRows)
    ->  append([':'(Owner, item, ref(ChildId), Index) | ChildRows],
               RestRows, Rows),
        Result = ok(Rows)
    ;   Result = RestResult
    ).

source_row_result(SourceRows, NodeId, Result) :-
    findall(
        Source,
        member(Source,
               SourceRows),
        Rows0),
    include(source_for(NodeId), Rows0, Rows),
    source_row_count_result(NodeId, Rows, Result).

source_for(NodeId, source(NodeId, _, _, _, _, _, _, _)).

source_row_count_result(_, [Source], ok(Source)) :- !.
source_row_count_result(NodeId, [],
                        error(diagnostic(
                                  syntax_graph, NodeId, missing_source))).
source_row_count_result(NodeId, Rows,
                        error(diagnostic(
                                  syntax_graph, NodeId,
                                  duplicate_source(Rows)))).
