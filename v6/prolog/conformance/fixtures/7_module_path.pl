:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% Module paths in FUNCTOR position. A decl reached at a path carries its
% segment list as rel_path_decl/2; the flat name is the segments joined by
% `__` and is what every phase past the dot phase reads.

% rx: the resolved head rel is one stream; the dotted spelling is a
% compile-time name, zero runtime rows.
fixture(module_path_in_head_refuses_by_name,
    prog(
        [ col_type(harvest/2, tree_id, int),
          col_type(harvest/2, picked, int),
          col_type(orchard__tree/2, tree_id, int),
          col_type(orchard__tree/2, picked, int),
          rel_path_decl(orchard__tree/2, [orchard, tree])
        ],
        [ (rel_path([orchard, tree], [TreeId, Picked]) <- harvest(TreeId, Picked)) ]),
    [],
    [
        [+harvest(1, 3)]
    ],
    [
        final(orchard__tree/2, [orchard__tree(1, 3)]),
        ticks(1)
    ]).

% rx: join against the resolved target's stream, identical to naming it flat.
fixture(module_path_in_body_refuses_by_name,
    prog(
        [ col_type(orchard__fruit/2, tree_id, int),
          col_type(orchard__fruit/2, picked, int),
          rel_path_decl(orchard__fruit/2, [orchard, fruit]),
          col_type(ripe/1, tree_id, int)
        ],
        [ (ripe(TreeId) <- rel_path([orchard, fruit], [TreeId, _Picked])) ]),
    [],
    [
        [+orchard__fruit(7, 2)]
    ],
    [
        final(ripe/1, [ripe(7)]),
        ticks(1)
    ]).

% `north` is an interior room no decl of its own names, so the walk has to
% mint it from the path and still find `tree` under it.
fixture(module_path_three_segments_keeps_every_segment,
    prog(
        [ col_type(orchard__north__tree/1, tree_id, int),
          rel_path_decl(orchard__north__tree/1, [orchard, north, tree]),
          col_type(leaf/1, tree_id, int)
        ],
        [ (leaf(TreeId) <- rel_path([orchard, north, tree], [TreeId])) ]),
    [],
    [
        [+orchard__north__tree(4)]
    ],
    [
        final(leaf/1, [leaf(4)]),
        ticks(1)
    ]).

% A bare local name and a path ending in the SAME segment are two rels: the
% local one binds first, the full path escapes to the other.
% rx: two independent streams, no shared subscription.
fixture(module_path_local_name_binds_before_the_dotted_one,
    prog(
        [ col_type(tree/1, tree_id, int),
          col_type(orchard__tree/1, tree_id, int),
          rel_path_decl(orchard__tree/1, [orchard, tree]),
          col_type(local_pick/1, tree_id, int),
          col_type(path_pick/1, tree_id, int)
        ],
        [ (local_pick(TreeId) <- tree(TreeId)),
          (path_pick(TreeId) <- rel_path([orchard, tree], [TreeId])) ]),
    [],
    [
        [+tree(1), +orchard__tree(2)]
    ],
    [
        final(local_pick/1, [local_pick(1)]),
        final(path_pick/1, [path_pick(2)]),
        ticks(1)
    ]).

% Nothing declares `orchard`, so the walk leaves the tree and every segment
% survives into the payload.
fixture(module_path_off_the_decl_tree_refuses,
    prog(
        [ col_type(leaf/1, tree_id, int)
        ],
        [ (leaf(TreeId) <- rel_path([orchard, north, tree], [TreeId])) ]),
    [],
    [],
    [
        throws(unsupported_construct(unresolvable_path([orchard, north, tree])))
    ]).

% The option phase runs at 5 and the dot phase at 44, so the mangle is already
% the flat name option expansion names its own minting after.
% rx: the tag join reads the option instance id, unchanged by the path.
fixture(module_path_and_option_column_coexist,
    prog(
        [ col_type(orchard__tree/2, tree_id, int),
          col_type(orchard__tree/2, label, option(text)),
          keyed(orchard__tree/2, [1]),
          rel_path_decl(orchard__tree/2, [orchard, tree]),
          col_type(labelled/2, tree_id, int),
          col_type(labelled/2, state, text)
        ],
        [ (labelled(TreeId, State) <-
              rel_path([orchard, tree], [TreeId, LabelOption]),
              '__opt_text_tag'(LabelOption, State)) ]),
    [],
    [
        [+'__opt_text_some'(801, "gala")],
        [+orchard__tree(1, 801)]
    ],
    [
        final(labelled/2, [labelled(1, some)]),
        ticks(2)
    ]).
