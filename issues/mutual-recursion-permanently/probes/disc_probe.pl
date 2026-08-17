% Two-way discriminator for the under-derivation: indirect (mutual) recursion
% versus a computed head column, one variable changed at a time.

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

fixture(mutual_recursion_no_head_expression,
  prog([ col_type(edge/2, from_node, text),
         col_type(edge/2, to_node, text),
         col_type(path/2, from_node, text),
         col_type(path/2, to_node, text),
         col_type(reach/2, from_node, text),
         col_type(reach/2, to_node, text) ],
       [ (path(FromNode, ToNode) <- edge(FromNode, ToNode)),
         (path(FromNode, ToNode) <-
            (reach(FromNode, MiddleNode), edge(MiddleNode, ToNode))),
         (reach(FromNode, ToNode) <- path(FromNode, ToNode)) ]),
  [],
  [ [ +edge(node_a, node_b), +edge(node_b, node_c), +edge(node_c, node_d) ] ],
  [ final(path/2, [ path(node_a, node_b), path(node_a, node_c),
                    path(node_a, node_d), path(node_b, node_c),
                    path(node_b, node_d), path(node_c, node_d) ]) ]).

fixture(direct_recursion_with_head_expression,
  prog([ col_type(type_row/3, id, int),
         col_type(type_row/3, kind, text),
         col_type(type_row/3, element_id, int),
         col_type(list_row/2, id, int),
         col_type(list_row/2, element_id, int),
         col_type(element_type/2, id, int),
         col_type(element_type/2, text, text) ],
       [ (list_row(Id, ElementId) <- type_row(Id, list, ElementId)),
         (element_type(Id, 'string') <- type_row(Id, primitive, _)),
         (element_type(Id, Text) <-
            (list_row(Id, ElementId), element_type(ElementId, ElementText),
             Text := concat([ElementText, '[]']))) ]),
  [],
  [ [ +type_row(1, primitive, 0), +type_row(2, list, 1),
      +type_row(3, list, 2), +type_row(4, list, 3) ] ],
  [ final(element_type/2, [ element_type(1, 'string'),
                            element_type(2, 'string[]'),
                            element_type(3, 'string[][]'),
                            element_type(4, 'string[][][]') ]) ]).
