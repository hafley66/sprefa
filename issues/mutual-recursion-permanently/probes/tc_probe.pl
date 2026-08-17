% Closure diameter 3 over edges that all arrive in ONE tick: a
% one-round-per-tick evaluator would need three ticks to settle.

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

fixture(tc_chain_batched_one_tick,
  prog([ col_type(edge/2, from_node, text),
         col_type(edge/2, to_node, text),
         col_type(path/2, from_node, text),
         col_type(path/2, to_node, text) ],
       [ (path(FromNode, ToNode) <- edge(FromNode, ToNode)),
         (path(FromNode, ToNode) <-
            (path(FromNode, MiddleNode), edge(MiddleNode, ToNode))) ]),
  [],
  [ [ +edge(node_a, node_b), +edge(node_b, node_c), +edge(node_c, node_d) ] ],
  [ final(path/2, [ path(node_a, node_b), path(node_a, node_c),
                    path(node_a, node_d), path(node_b, node_c),
                    path(node_b, node_d), path(node_c, node_d) ]) ]).

% Same rules, one edge per tick: the staged control the corpus already has.
fixture(tc_chain_one_edge_per_tick,
  prog([ col_type(edge/2, from_node, text),
         col_type(edge/2, to_node, text),
         col_type(path/2, from_node, text),
         col_type(path/2, to_node, text) ],
       [ (path(FromNode, ToNode) <- edge(FromNode, ToNode)),
         (path(FromNode, ToNode) <-
            (path(FromNode, MiddleNode), edge(MiddleNode, ToNode))) ]),
  [],
  [ [ +edge(node_a, node_b) ],
    [ +edge(node_b, node_c) ],
    [ +edge(node_c, node_d) ] ],
  [ final(path/2, [ path(node_a, node_b), path(node_a, node_c),
                    path(node_a, node_d), path(node_b, node_c),
                    path(node_b, node_d), path(node_c, node_d) ]) ]).
