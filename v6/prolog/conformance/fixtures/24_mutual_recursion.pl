% FAIL-FIRST RECEIPT (measured on this tree before the outer-round loop
% landed, sweep stage 3): both fixtures compiled and both replayed WRONG on
% the emitted ts door and clean-FAIL on the emitted rust door, while the
% conformance oracle passed both. Receipts in the arc report.
%
% What the corpus was missing: engine_core.pl:mutual_recursion_matches_oracle
% is a depth-1 even/odd cycle, so ONE dependency-ordered statement pass closes
% it and the single-pass emitted loop looked correct. Neither fixture below
% closes in one pass: each needs the stratum's statement list re-run until no
% statement adds a row.

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% Two heads on one cycle, neither reading ITSELF, so the direct-self-read test
% in lower.pl:rules_read_head_recursively/2 leaves both with ExpandPlan=none
% and the closure has to come from the outer rounds.
fixture(mutual_closure_needs_outer_rounds,
  prog([ col_type(edge/2, from_node, int),
         col_type(edge/2, to_node, int),
         col_type(path/2, from_node, int),
         col_type(path/2, to_node, int),
         col_type(reach/2, from_node, int),
         col_type(reach/2, to_node, int) ],
       [ (path(FromNode, ToNode) <- edge(FromNode, ToNode)),
         (path(FromNode, ToNode) <- reach(FromNode, ToNode)),
         (reach(FromNode, ToNode) <-
            (path(FromNode, MiddleNode), edge(MiddleNode, ToNode))) ]),
  [],
  [ [ +edge(1, 2), +edge(2, 3), +edge(3, 4) ] ],
  [ final(path/2, [ path(1, 2), path(1, 3), path(1, 4),
                    path(2, 3), path(2, 4), path(3, 4) ]),
    final(reach/2, [ reach(1, 3), reach(1, 4), reach(2, 4) ]) ]).

% The dl6-first typegen shape: a list type carries its element type, the
% element type of a list type is itself a list type, and the pair walks the
% nesting ladder. Four LEVELS (0..3) only exist if the cycle runs four rounds.
fixture(typegen_list_element_ladder,
  prog([ col_type(list_of/2, list_type_id, int),
         col_type(list_of/2, element_type_id, int),
         col_type(root_type/1, type_id, int),
         col_type(list_type/2, type_id, int),
         col_type(list_type/2, level, int),
         col_type(element_type/2, type_id, int),
         col_type(element_type/2, level, int) ],
       [ (list_type(TypeId, 0) <- root_type(TypeId)),
         (element_type(ElementTypeId, NextLevel) <-
            (list_type(ListTypeId, ListLevel),
             list_of(ListTypeId, ElementTypeId),
             NextLevel := ListLevel + 1)),
         (list_type(TypeId, Level) <-
            (element_type(TypeId, Level), list_of(TypeId, _AnyElement))) ]),
  [],
  [ [ +root_type(10),
      +list_of(10, 11), +list_of(11, 12), +list_of(12, 13) ] ],
  [ final(list_type/2, [ list_type(10, 0), list_type(11, 1), list_type(12, 2) ]),
    final(element_type/2, [ element_type(11, 1), element_type(12, 2),
                            element_type(13, 3) ]) ]).
