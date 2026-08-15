% 22_ref_column_collision.pl : a ref column whose NAME also names a column of
% the referenced type.
%
% FAIL-PRE-FIX (docs/failure-modes.md entry 52, traced from b32499c5):
% lower.pl dictionary_render_expr/3 wrote the outer row's column UNQUALIFIED
% inside its correlated render subquery, so SQLite bound the name to the child
% `__ref_` view, the `d."__id" = <child column>` comparison matched nothing and
% the emitted door rendered null where the oracle rendered the child tree.
% Generics-free on purpose: two plain type_decls reproduce it, so the defect is
% the render expression and not the template plane.

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% Both outer columns collide: `first` and `second` name columns of the child
% type as well as of the parent.
fixture(colliding_ref_column_names_render_the_child_tree,
  prog([ type_decl(inner_pair, [col(first, int), col(second, int)]),
         col_type(inner_pair/2, first, int),
         col_type(inner_pair/2, second, int),
         type_decl(outer_pair, [col(first, inner_pair), col(second, inner_pair)]),
         col_type(outer_pair/2, first, inner_pair),
         col_type(outer_pair/2, second, inner_pair),
         col_type(holder/2, id, int),
         col_type(holder/2, nested, outer_pair) ],
       [ (touched(Id) <- holder(Id, _Nested)) ]),
  [],
  [ [ +holder(1, obj([first-obj([first-10, second-20]),
                      second-obj([first-30, second-40])])) ] ],
  [ final(touched/1, [ touched(1) ]),
    final(holder/2, [ holder(1, obj([first-obj([first-10, second-20]),
                                     second-obj([first-30, second-40])])) ]),
    ticks(1) ]).

% The control: the same two levels with DISJOINT column names never bound to
% the child view, so this one was green before the fix and stays green.
fixture(disjoint_ref_column_names_render_the_child_tree,
  prog([ type_decl(leaf_pair, [col(left, int), col(right, int)]),
         col_type(leaf_pair/2, left, int),
         col_type(leaf_pair/2, right, int),
         type_decl(shell_pair, [col(head, leaf_pair), col(tail, leaf_pair)]),
         col_type(shell_pair/2, head, leaf_pair),
         col_type(shell_pair/2, tail, leaf_pair),
         col_type(carrier/2, id, int),
         col_type(carrier/2, nested, shell_pair) ],
       [ (seen(Id) <- carrier(Id, _Nested)) ]),
  [],
  [ [ +carrier(1, obj([head-obj([left-10, right-20]),
                       tail-obj([left-30, right-40])])) ] ],
  [ final(seen/1, [ seen(1) ]),
    final(carrier/2, [ carrier(1, obj([head-obj([left-10, right-20]),
                                       tail-obj([left-30, right-40])])) ]),
    ticks(1) ]).

% One colliding name beside a disjoint sibling: the sibling column was always
% right, so a fix that only moved the whole row would not be visible here.
fixture(one_colliding_ref_column_beside_a_disjoint_sibling,
  prog([ type_decl(point_pair, [col(first, int), col(depth, int)]),
         col_type(point_pair/2, first, int),
         col_type(point_pair/2, depth, int),
         type_decl(mixed_pair, [col(first, point_pair), col(label, text)]),
         col_type(mixed_pair/2, first, point_pair),
         col_type(mixed_pair/2, label, text),
         col_type(record/2, id, int),
         col_type(record/2, nested, mixed_pair) ],
       [ (kept(Id) <- record(Id, _Nested)) ]),
  [],
  [ [ +record(1, obj([first-obj([first-7, depth-8]), label-"edge"])) ] ],
  [ final(kept/1, [ kept(1) ]),
    final(record/2, [ record(1, obj([first-obj([depth-8, first-7]),
                                     label-"edge"])) ]),
    ticks(1) ]).
