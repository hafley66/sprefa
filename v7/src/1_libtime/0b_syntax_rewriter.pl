:- module(dl7_syntax_rewriter,
          [ active_nodes/2,
            rewrite_active_graph/8
          ]).

active_nodes(Rows, Nodes) :-
    findall(Node, member(syntax_frontier(_, Node), Rows), Roots0),
    sort(Roots0, Roots),
    reachable_nodes(Roots, Rows, [], Nodes0),
    sort(Nodes0, Nodes).

reachable_nodes([], _, Seen, Seen).
reachable_nodes([Node | Queue], Rows, Seen0, Seen) :-
    (   memberchk(Node, Seen0)
    ->  reachable_nodes(Queue, Rows, Seen0, Seen)
    ;   findall(Child,
                member(':'(Node, item, ref(Child), _), Rows),
                Children),
        append(Queue, Children, NextQueue),
        reachable_nodes(NextQueue, Rows, [Node | Seen0], Seen)
    ).

rewrite_active_graph(Rows, AvailableRows, Claims, Outputs, Wave,
                     NextRows, Provenance, Diagnostics) :-
    ordered_frontier(Rows, Roots),
    rewrite_sequence(Roots, Rows, AvailableRows, Claims, Outputs, Wave,
                     NewRoots, Overrides, Provenance0, RewriteDiagnostics),
    (   RewriteDiagnostics == []
    ->  apply_rewrite(Rows, AvailableRows, NewRoots, Overrides, NextRows),
        sort(Provenance0, Provenance),
        Diagnostics = []
    ;   NextRows = [],
        Provenance = [],
        Diagnostics = RewriteDiagnostics
    ).

ordered_frontier(Rows, Roots) :-
    findall(Index-Node, member(syntax_frontier(Index, Node), Rows), Pairs0),
    keysort(Pairs0, Pairs),
    pairs_values(Pairs, Roots).

ordered_children(Rows, Owner, Children) :-
    findall(Index-Child,
            member(':'(Owner, item, ref(Child), Index), Rows), Pairs0),
    keysort(Pairs0, Pairs),
    pairs_values(Pairs, Children).

pairs_values([], []).
pairs_values([_-Value | Pairs], [Value | Values]) :-
    pairs_values(Pairs, Values).

rewrite_sequence([], _, _, _, _, _, [], [], [], []).
rewrite_sequence([Node | Nodes], Rows, Available, Claims, Outputs, Wave,
                 Rewritten, Overrides, Provenance, Diagnostics) :-
    rewrite_node(Node, Rows, Available, Claims, Outputs, Wave,
                 OwnNodes, OwnOverrides, OwnProvenance, OwnDiagnostics),
    rewrite_sequence(Nodes, Rows, Available, Claims, Outputs, Wave,
                     RestNodes, RestOverrides, RestProvenance,
                     RestDiagnostics),
    append(OwnNodes, RestNodes, Rewritten),
    append(OwnOverrides, RestOverrides, Overrides),
    append(OwnProvenance, RestProvenance, Provenance),
    append(OwnDiagnostics, RestDiagnostics, Diagnostics).

rewrite_node(Node, _, Available, Claims, Outputs, Wave,
             Rewritten, [], Provenance, Diagnostics) :-
    findall(Macro, member(claim(Node, Macro), Claims), Macros),
    Macros \== [],
    !,
    claimed_node_result(Node, Macros, Available, Outputs, Wave,
                        Rewritten, Provenance, Diagnostics).
rewrite_node(Node, Rows, Available, Claims, Outputs, Wave,
             [Node], Overrides, Provenance, Diagnostics) :-
    (   memberchk(syntax_form(Node), Rows)
    ->  ordered_children(Rows, Node, Children),
        rewrite_sequence(Children, Rows, Available, Claims, Outputs, Wave,
                         NewChildren, ChildOverrides, Provenance,
                         Diagnostics),
        Overrides = [children(Node, NewChildren) | ChildOverrides]
    ;   Overrides = [],
        Provenance = [],
        Diagnostics = []
    ).

claimed_node_result(Node, [Macro], Available, Outputs, Wave,
                    Rewritten, Provenance, Diagnostics) :-
    !,
    findall(Ordinal-Output,
            member(output(Node, Output, Ordinal), Outputs), Pairs0),
    keysort(Pairs0, Pairs),
    validate_output_pairs(Node, Pairs, Available, PairDiagnostics),
    (   PairDiagnostics == []
    ->  pairs_values(Pairs, Rewritten),
        output_provenance(Pairs, Node, Macro, Wave, OutputProvenance),
        Provenance = [expansion_claim(Node, Macro, Wave)
                      | OutputProvenance],
        Diagnostics = []
    ;   Rewritten = [],
        Provenance = [],
        Diagnostics = PairDiagnostics
    ).
claimed_node_result(Node, Macros, _, _, _, [], [],
                    [diagnostic(macrotime, Node,
                                conflicting_macro_claims(Macros))]).

validate_output_pairs(Node, Pairs, Available, Diagnostics) :-
    pairs_ordinals(Pairs, Ordinals),
    length(Pairs, Count),
    numlist_or_empty(0, Count, Expected),
    findall(diagnostic(macrotime, Node,
                       unknown_expansion_output(Output)),
            ( member(_-Output, Pairs),
              \+ syntax_row_for_node(Available, Output)
            ),
            UnknownDiagnostics),
    (   Ordinals == Expected
    ->  DenseDiagnostics = []
    ;   DenseDiagnostics = [diagnostic(
                                macrotime, Node,
                                non_dense_expansion_ordinals(Ordinals))]
    ),
    append(DenseDiagnostics, UnknownDiagnostics, Diagnostics).

pairs_ordinals([], []).
pairs_ordinals([Ordinal-_ | Pairs], [Ordinal | Ordinals]) :-
    pairs_ordinals(Pairs, Ordinals).

numlist_or_empty(_, 0, []) :- !.
numlist_or_empty(Start, Count, Values) :-
    End is Count - 1,
    numlist(Start, End, Values).

syntax_row_for_node(Rows, Node) :-
    ( memberchk(syntax_form(Node), Rows)
    ; memberchk(syntax_atom(Node, _), Rows)
    ; memberchk(syntax_literal(Node, _), Rows)
    ; memberchk(syntax_variable(Node, _, _), Rows)
    ).

output_provenance([], _, _, _, []).
output_provenance([Ordinal-Output | Pairs], Node, Macro, Wave,
                  [expansion_output(Node, Macro, Wave, Output, Ordinal)
                   | Provenance]) :-
    output_provenance(Pairs, Node, Macro, Wave, Provenance).

apply_rewrite(Rows, AvailableRows, Roots, Overrides, NextRows) :-
    append(Rows, AvailableRows, AllRows0),
    sort(AllRows0, AllRows),
    exclude(frontier_row, AllRows, WithoutFrontier),
    exclude(overridden_edge(Overrides), WithoutFrontier, RetainedRows),
    frontier_rows(Roots, 0, FrontierRows),
    override_rows(Overrides, OverrideRows),
    append([RetainedRows, FrontierRows, OverrideRows], CandidateRows0),
    sort(CandidateRows0, CandidateRows),
    active_nodes(CandidateRows, Reachable),
    include(row_reachable(Reachable), CandidateRows, ReachableRows),
    sort(ReachableRows, NextRows).

frontier_row(syntax_frontier(_, _)).

overridden_edge(Overrides, ':'(Owner, item, _, _)) :-
    memberchk(children(Owner, _), Overrides).

frontier_rows([], _, []).
frontier_rows([Node | Nodes], Index,
              [syntax_frontier(Index, Node) | Rows]) :-
    NextIndex is Index + 1,
    frontier_rows(Nodes, NextIndex, Rows).

override_rows([], []).
override_rows([children(Owner, Children) | Overrides], Rows) :-
    child_edge_rows(Children, Owner, 0, ChildRows),
    override_rows(Overrides, RestRows),
    append(ChildRows, RestRows, Rows).

child_edge_rows([], _, _, []).
child_edge_rows([Child | Children], Owner, Index,
                [':'(Owner, item, ref(Child), Index) | Rows]) :-
    NextIndex is Index + 1,
    child_edge_rows(Children, Owner, NextIndex, Rows).

row_reachable(_, syntax_frontier(_, _)).
row_reachable(Nodes, node(Node)) :- memberchk(Node, Nodes).
row_reachable(Nodes, syntax_form(Node)) :- memberchk(Node, Nodes).
row_reachable(Nodes, syntax_atom(Node, _)) :- memberchk(Node, Nodes).
row_reachable(Nodes, syntax_literal(Node, _)) :- memberchk(Node, Nodes).
row_reachable(Nodes, syntax_variable(Node, _, _)) :- memberchk(Node, Nodes).
row_reachable(Nodes, source(Node, _, _, _, _, _, _, _)) :-
    memberchk(Node, Nodes).
row_reachable(Nodes, ':'(Owner, item, ref(Target), _)) :-
    memberchk(Owner, Nodes),
    memberchk(Target, Nodes).
