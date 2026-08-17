% The typegen REPORT's pre-unroll shape: list_type / element_type mutual
% recursion over a growing text, nesting depth 3, every row in ONE tick.

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

fixture(list_type_self_recursive_one_tick,
  prog([ col_type(type_row/3, id, int),
         col_type(type_row/3, kind, text),
         col_type(type_row/3, element_id, int),
         col_type(leaf_type/2, id, int),
         col_type(leaf_type/2, text, text),
         col_type(list_row/2, id, int),
         col_type(list_row/2, element_id, int),
         col_type(list_type/2, id, int),
         col_type(list_type/2, text, text),
         col_type(element_type/2, id, int),
         col_type(element_type/2, text, text) ],
       [ (leaf_type(Id, 'string') <- type_row(Id, primitive, _)),
         (list_row(Id, ElementId) <- type_row(Id, list, ElementId)),
         (list_type(Id, Text) <-
            (list_row(Id, ElementId), element_type(ElementId, ElementText),
             Text := concat([ElementText, '[]']))),
         (element_type(Id, Text) <- leaf_type(Id, Text)),
         (element_type(Id, Text) <- list_type(Id, Text)) ]),
  [],
  [ [ +type_row(1, primitive, 0), +type_row(2, list, 1),
      +type_row(3, list, 2), +type_row(4, list, 3) ] ],
  [ final(list_type/2, [ list_type(2, 'string[]'),
                         list_type(3, 'string[][]'),
                         list_type(4, 'string[][][]') ]) ]).
