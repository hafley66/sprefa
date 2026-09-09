:- module(dl7_syntax_materializer, [materialize_syntax/4]).

:- use_module(library(error), [must_be/2]).

%% materialize_syntax(+SyntaxGraphRows,
%%                    -ReaderForms, -SourceRows, -Diagnostics) is det.
%
% Reconstruct the current active syntax frontier for the existing tree
% lowerer. Identities survive unchanged. Each occurrence must appear once in
% the active tree, own one node row, one payload alternative, one source row,
% and a dense child sequence when it is a form.
materialize_syntax(Rows, Forms, SourceRows, Diagnostics) :-
    must_be(list, Rows),
    must_be(ground, Rows),
    ordered_nodes(Rows, syntax_frontier, none, Roots, FrontierDiagnostics),
    materialize_after_frontier(FrontierDiagnostics, Roots, Rows,
                               Forms, SourceRows, Diagnostics).

materialize_after_frontier([], Roots, Rows, Forms, SourceRows, Diagnostics) :-
    !,
    materialize_nodes(Roots, Rows, [], _, Forms, SourceRows, Diagnostics).
materialize_after_frontier(Diagnostics, _, _, [], [], Diagnostics).

materialize_nodes([], _, Seen, Seen, [], [], []).
materialize_nodes([Node | Nodes], Rows, Seen0, Seen,
                  Forms, SourceRows, Diagnostics) :-
    materialize_node(Node, Rows, Seen0, Seen1,
                     NodeResult, NodeSources, NodeDiagnostics),
    materialize_nodes(Nodes, Rows, Seen1, Seen,
                      RestForms, RestSources, RestDiagnostics),
    append_node_result(NodeResult, RestForms, Forms),
    append(NodeSources, RestSources, SourceRows),
    append(NodeDiagnostics, RestDiagnostics, Diagnostics).

append_node_result(none, Nodes, Nodes).
append_node_result(node(Node), Nodes, [Node | Nodes]).

materialize_node(Node, _, Seen, Seen, none, [],
                 [diagnostic(syntax_graph, Node,
                             reused_syntax_occurrence)]) :-
    memberchk(Node, Seen),
    !.
materialize_node(Node, Rows, Seen0, Seen, Result, SourceRows, Diagnostics) :-
    node_presence_diagnostics(Rows, Node, NodeDiagnostics),
    payload_result(Rows, Node, PayloadResult, PayloadDiagnostics),
    source_result(Rows, Node, SourceResult, SourceDiagnostics),
    materialize_payload(PayloadResult, Node, Rows, [Node | Seen0], Seen,
                        Payload, ChildSources, ChildDiagnostics),
    append([NodeDiagnostics, PayloadDiagnostics, SourceDiagnostics,
            ChildDiagnostics], Diagnostics),
    materialized_node_result(Diagnostics, Node, Payload, SourceResult,
                             Result, SourceRows0),
    append(SourceRows0, ChildSources, SourceRows).

node_presence_diagnostics(Rows, Node, Diagnostics) :-
    findall(node(Node), member(node(Node), Rows), Matches),
    exact_one_diagnostics(Matches, syntax_node, Node, Diagnostics).

payload_result(Rows, Node, Result, Diagnostics) :-
    findall(Payload, syntax_payload(Rows, Node, Payload), Payloads0),
    sort(Payloads0, Payloads),
    (   Payloads = [Payload]
    ->  Result = ok(Payload),
        Diagnostics = []
    ;   Result = error,
        Diagnostics = [diagnostic(
                           syntax_graph, Node,
                           payload_alternatives(Payloads))]
    ).

syntax_payload(Rows, Node, form) :-
    member(syntax_form(Node), Rows).
syntax_payload(Rows, Node, atom(Name)) :-
    member(syntax_atom(Node, Name), Rows).
syntax_payload(Rows, Node, literal(Value)) :-
    member(syntax_literal(Node, Value), Rows).
syntax_payload(Rows, Node, variable(Variable, Name)) :-
    member(syntax_variable(Node, Variable, Name), Rows).

source_result(Rows, Node, Result, Diagnostics) :-
    findall(Source,
            ( member(Source, Rows),
              Source = source(Node, _, _, _, _, _, _, _)
            ),
            Sources),
    (   Sources = [Source]
    ->  Result = ok(Source),
        Diagnostics = []
    ;   Result = error,
        Diagnostics = [diagnostic(
                           syntax_graph, Node,
                           source_rows(Sources))]
    ).

exact_one_diagnostics([_], _, _, []) :- !.
exact_one_diagnostics(Matches, Kind, Node,
                      [diagnostic(syntax_graph, Node,
                                  expected_one(Kind, Matches))]).

materialize_payload(error, _, _, Seen, Seen, invalid, [], []).
materialize_payload(ok(atom(Name)), _, _, Seen, Seen,
                    atom(Name), [], []).
materialize_payload(ok(literal(Value)), _, _, Seen, Seen,
                    literal(Value), [], []).
materialize_payload(ok(variable(Variable, Name)), _, _, Seen, Seen,
                    variable(Variable, Name), [], []).
materialize_payload(ok(form), Node, Rows, Seen0, Seen,
                    form(Children), SourceRows, Diagnostics) :-
    ordered_nodes(Rows, item, Node, ChildIds, EdgeDiagnostics),
    materialize_nodes(ChildIds, Rows, Seen0, Seen,
                      Children, SourceRows, ChildDiagnostics),
    append(EdgeDiagnostics, ChildDiagnostics, Diagnostics).

materialized_node_result([], Node, Payload, ok(Source),
                         node(node(Node, Payload)), [Source]) :- !.
materialized_node_result(_, _, _, _, none, []).

ordered_nodes(Rows, Label, Owner, Nodes, Diagnostics) :-
    findall(Index-Node,
            indexed_node(Rows, Label, Owner, Index, Node), Pairs0),
    keysort(Pairs0, Pairs),
    pair_parts(Pairs, Indices, Nodes),
    length(Pairs, Count),
    expected_indices(Count, Expected),
    (   Indices == Expected
    ->  Diagnostics = []
    ;   Diagnostics = [diagnostic(
                           syntax_graph, Owner,
                           non_dense(Label, Indices))]
    ).

indexed_node(Rows, syntax_frontier, none, Index, Node) :-
    member(syntax_frontier(Index, Node), Rows).
indexed_node(Rows, item, Owner, Index, Node) :-
    member(':'(Owner, item, ref(Node), Index), Rows).

pair_parts([], [], []).
pair_parts([Index-Node | Pairs], [Index | Indices], [Node | Nodes]) :-
    pair_parts(Pairs, Indices, Nodes).

expected_indices(0, []) :- !.
expected_indices(Count, Indices) :-
    End is Count - 1,
    numlist(0, End, Indices).
