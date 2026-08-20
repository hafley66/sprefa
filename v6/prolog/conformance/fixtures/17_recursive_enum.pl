:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% One enum, two disjoint id ranges, one tag join. The branch variant's
% left/right fields are typed by the enum itself and arrive as the referenced
% instance ids, so a branch is a 3-column row.
%   ids 1-3    an acyclic tree: the tag join tracks every retraction.
%   ids 11-19  a branch whose left is its own id, and a two-row mutual cycle:
%              the tag join never recurses into the id fields, so both cycles
%              are stored and rendered on the same tick instead of looping.
% folded 2026-08-20 from recursive_enum_acyclic_tree_round_trips,
% recursive_enum_cyclic_values_store_and_render.
fixture(recursive_enum_tree_and_cycles_round_trip,
    prog(
        [enum_decl(tree, (leaf(value: int) ; branch(left: tree, right: tree))),
         col_type(tree_kind/2, id, int),
         col_type(tree_kind/2, kind, text)],
        [(tree_kind(Id, Kind) <- tree_tag(Id, Kind))]),
    [],
    [
        [+tree_leaf(1, 5), +tree_leaf(3, 7)],
        [+tree_branch(2, 1, 3)],
        [-tree_leaf(1, 5)],
        [+tree_leaf(11, 5)],
        [+tree_branch(19, 19, 11)],
        [+tree_branch(12, 13, 11), +tree_branch(13, 12, 11)]
    ],
    [
        final(tree_leaf/2, [tree_leaf(3, 7), tree_leaf(11, 5)]),
        final(tree_branch/3,
              [tree_branch(2, 1, 3), tree_branch(12, 13, 11),
               tree_branch(13, 12, 11), tree_branch(19, 19, 11)]),
        final(tree_tag/2,
              [tree_tag(2, branch), tree_tag(3, leaf), tree_tag(11, leaf),
               tree_tag(12, branch), tree_tag(13, branch), tree_tag(19, branch)]),
        final(tree_kind/2,
              [tree_kind(2, branch), tree_kind(3, leaf), tree_kind(11, leaf),
               tree_kind(12, branch), tree_kind(13, branch), tree_kind(19, branch)]),
        deltas(tree_tag/2, [
            [+tree_tag(1, leaf), +tree_tag(3, leaf)],
            [+tree_tag(2, branch)],
            [-tree_tag(1, leaf)],
            [+tree_tag(11, leaf)],
            [+tree_tag(19, branch)],
            [+tree_tag(12, branch), +tree_tag(13, branch)]
        ]),
        ticks(6)
    ]).

