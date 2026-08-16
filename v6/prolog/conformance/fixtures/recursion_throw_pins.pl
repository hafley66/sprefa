% recursion_throw_pins.pl : the oracle (engine.pl) and the compiler (lower.pl)
% must throw the SAME terms on the same direct recursive spellings (PR #266 class).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% A recursive head read twice in its own body throws at
% check_single_recursive_read (lower.pl:5205): the CTE has no place to
% keep the two reads in step.
fixture(recursive_cte_multiple_self_reads_is_refused,
  prog([ col_type(edge/2, from_node, int),
         col_type(edge/2, to_node, int),
         col_type(reachable/2, from_node, int),
         col_type(reachable/2, to_node, int) ],
       [ (reachable(FromNode, ToNode) <- edge(FromNode, ToNode)),
         (reachable(FromNode, ToNode) <-
            (reachable(FromNode, MiddleNode),
             reachable(MiddleNode, ToNode))) ]),
  [],
  [ [ +edge(1, 2) ] ],
  [ throws(unsupported_construct(
             recursive_cte_multiple_self_reads(reachable/2, 2))) ]).

% A text built directly in a recursive head throws at
% recursive_arm_builds_no_string (lower.pl:5260); the concat in the head is
% the built value.
fixture(built_text_in_recursive_head_is_refused,
  prog([ col_type(seed_word/1, word, text),
         col_type(chain/1, word, text) ],
       [ (chain(Word) <- seed_word(Word)),
         (chain(concat([Word, '/'])) <-
            (chain(Word), seed_word(_))) ]),
  [],
  [ [ +seed_word('a') ] ],
  [ throws(unsupported_construct(
             built_text_in_recursive_head(chain/1))) ]).

% A list built directly in a recursive head throws at
% recursive_arm_builds_no_list (lower.pl:5264); the split in the head builds
% the list.
fixture(built_list_in_recursive_head_is_refused,
  prog([ col_type(edge/2, from_node, int),
         col_type(edge/2, to_node, int),
         col_type(mark/2, node_id, int),
         col_type(mark/2, parts, list(text)) ],
       [ (mark(NodeId, split("y", ",")) <-
            (edge(NodeId, FromNode), mark(FromNode, _))) ]),
  [],
  [ [ +edge(1, 2) ] ],
  [ throws(unsupported_construct(
             built_list_in_recursive_head(mark/2))) ]).

% A COMPILING recursive fixture whose lowering passes through both build-guard
% success arms (recursive_arm_builds_no_string(_, []) / no_list(_, [])): no
% text or list is built in the head, so neither guard fires. The closure
% closes across ticks as link rows arrive.
fixture(recursive_closure_passes_both_build_guard_arms,
  prog([ col_type(link/2, from_node, int),
         col_type(link/2, to_node, int),
         col_type(reach/2, from_node, int),
         col_type(reach/2, to_node, int) ],
       [ (reach(FromNode, ToNode) <- link(FromNode, ToNode)),
         (reach(FromNode, ToNode) <-
            (reach(FromNode, MiddleNode), link(MiddleNode, ToNode))) ]),
  [],
  [ [ +link(1, 2) ],
    [ +link(2, 3) ] ],
  [ final(reach/2, [ reach(1, 2), reach(1, 3), reach(2, 3) ]) ]).
