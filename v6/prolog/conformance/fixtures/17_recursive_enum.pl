:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% A recursive enum: the branch variant's left/right fields are typed by the
% enum itself and arrive as the referenced instance ids, so a branch is a
% 3-column row and the tag join still tracks every retraction.
fixture(recursive_enum_acyclic_tree_round_trips,
    prog(
        [enum_decl(tree, (leaf(value: int) ; branch(left: tree, right: tree))),
         col_type(tree_kind/2, id, int),
         col_type(tree_kind/2, kind, text)],
        [(tree_kind(Id, Kind) <- tree_tag(Id, Kind))]),
    [],
    [
        [+tree_leaf(1, 5), +tree_leaf(3, 7)],
        [+tree_branch(2, 1, 3)],
        [-tree_leaf(1, 5)]
    ],
    [
        final(tree_leaf/2, [tree_leaf(3, 7)]),
        final(tree_branch/3, [tree_branch(2, 1, 3)]),
        final(tree_tag/2, [tree_tag(2, branch), tree_tag(3, leaf)]),
        final(tree_kind/2, [tree_kind(2, branch), tree_kind(3, leaf)]),
        deltas(tree_tag/2, [
            [+tree_tag(1, leaf), +tree_tag(3, leaf)],
            [+tree_tag(2, branch)],
            [-tree_tag(1, leaf)]
        ]),
        ticks(3)
    ]).

% A branch whose left is its own id, and a two-row mutual cycle: the enum's
% tag join never recurses into the id fields, so both cycles are stored and
% rendered on the same tick instead of looping.
fixture(recursive_enum_cyclic_values_store_and_render,
    prog(
        [enum_decl(tree, (leaf(value: int) ; branch(left: tree, right: tree))),
         col_type(tree_kind/2, id, int),
         col_type(tree_kind/2, kind, text)],
        [(tree_kind(Id, Kind) <- tree_tag(Id, Kind))]),
    [],
    [
        [+tree_leaf(1, 5)],
        [+tree_branch(9, 9, 1)],
        [+tree_branch(2, 3, 1), +tree_branch(3, 2, 1)]
    ],
    [
        final(tree_leaf/2, [tree_leaf(1, 5)]),
        final(tree_branch/3,
              [tree_branch(2, 3, 1), tree_branch(3, 2, 1), tree_branch(9, 9, 1)]),
        final(tree_tag/2,
              [tree_tag(1, leaf), tree_tag(2, branch), tree_tag(3, branch), tree_tag(9, branch)]),
        final(tree_kind/2,
              [tree_kind(1, leaf), tree_kind(2, branch), tree_kind(3, branch), tree_kind(9, branch)]),
        deltas(tree_tag/2, [
            [+tree_tag(1, leaf)],
            [+tree_tag(9, branch)],
            [+tree_tag(2, branch), +tree_tag(3, branch)]
        ]),
        ticks(3)
    ]).
