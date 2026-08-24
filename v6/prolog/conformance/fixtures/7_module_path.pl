:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% Module paths in FUNCTOR position. A decl reached at a path carries its
% segment list as rel_path_decl/2; the flat name is the segments joined by
% `__` and is what every phase past the dot phase reads.

% rx: the resolved head rel is one stream; the dotted spelling is a
% compile-time name, zero runtime rows.
fixture(module_path_in_head_resolves_and_contributes,
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
fixture(module_path_in_body_reads_the_flat_rel,
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
fixture(module_path_three_segments_resolve_through_the_rooms,
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

% ── nesting: name paths preserve authored relation shapes ────────────────────

fixture(nested_child_uses_its_authored_arity,
    prog(
        [ col_type(orchard/1, orchard_id, int),
          col_type(orchard__tree/1, tree_id, int),
          rel_path_decl(orchard__tree/1, [orchard, tree]),
          col_type(planted/2, orchard_id, int),
          col_type(planted/2, tree_id, int)
        ],
        [ (orchard(OrchardId) <- planted(OrchardId, _)),
          (orchard__tree(TreeId) <- orchard(Oid), planted(Oid, TreeId)) ]),
    [],
    [
        [+planted(1, 7)]
    ],
    [
        final(orchard/1, [orchard(1)]),
        final(orchard__tree/1, [orchard__tree(7)]),
        ticks(1)
    ]).

fixture(nested_two_parent_rows_share_the_flat_child_relation,
    prog(
        [ col_type(orchard/1, orchard_id, int),
          col_type(orchard__tree/1, tree_id, int),
          rel_path_decl(orchard__tree/1, [orchard, tree]),
          col_type(planted/2, orchard_id, int),
          col_type(planted/2, tree_id, int),
          col_type(per_orchard/2, orchard_id, int),
          col_type(per_orchard/2, trees, int)
        ],
        [ (orchard(OrchardId) <- planted(OrchardId, _)),
          (orchard__tree(TreeId) <- orchard(Oid), planted(Oid, TreeId)),
          (per_orchard(GroupId, count(TreeId2)) <-
              planted(GroupId, TreeId2)) ]),
    [],
    [
        [+planted(1, 7), +planted(1, 8), +planted(2, 9)]
    ],
    [
        final(orchard/1, [orchard(1), orchard(2)]),
        final(orchard__tree/1, [orchard__tree(7), orchard__tree(8),
                                orchard__tree(9)]),
        final(per_orchard/2, [per_orchard(1, 2), per_orchard(2, 1)]),
        ticks(1)
    ]).

fixture(nested_parent_rows_do_not_gate_the_child,
    prog(
        [ col_type(orchard/1, orchard_id, int),
          col_type(orchard__tree/1, tree_id, int),
          rel_path_decl(orchard__tree/1, [orchard, tree]),
          col_type(planted/2, orchard_id, int),
          col_type(planted/2, tree_id, int),
          col_type(seeded/2, orchard_id, int),
          col_type(seeded/2, tree_id, int)
        ],
        [ (orchard(OrchardId) <- planted(OrchardId, _)),
          (orchard__tree(TreeId) <- seeded(_, TreeId)) ]),
    [],
    [
        [+seeded(1, 7)]
    ],
    [
        final(orchard/1, []),
        final(orchard__tree/1, [orchard__tree(7)]),
        ticks(1)
    ]).

fixture(nested_body_atom_uses_its_authored_arity,
    prog(
        [ col_type(orchard/1, orchard_id, int),
          col_type(orchard__tree/1, tree_id, int),
          rel_path_decl(orchard__tree/1, [orchard, tree]),
          col_type(planted/2, orchard_id, int),
          col_type(planted/2, tree_id, int),
          col_type(any_tree/1, tree_id, int)
        ],
        [ (orchard(OrchardId) <- planted(OrchardId, _)),
          (orchard__tree(TreeId) <- orchard(Oid), planted(Oid, TreeId)),
          (any_tree(TreeId2) <- orchard__tree(TreeId2)) ]),
    [],
    [
        [+planted(1, 7), +planted(2, 9)]
    ],
    [
        final(any_tree/1, [any_tree(7), any_tree(9)]),
        ticks(1)
    ]).

fixture(nested_head_without_a_parent_binding_derives,
    prog(
        [ col_type(orchard/1, orchard_id, int),
          col_type(orchard__tree/1, tree_id, int),
          rel_path_decl(orchard__tree/1, [orchard, tree]),
          col_type(planted/2, orchard_id, int),
          col_type(planted/2, tree_id, int)
        ],
        [ (orchard(OrchardId) <- planted(OrchardId, _)),
          (orchard__tree(TreeId) <- planted(_, TreeId)) ]),
    [],
    [
        [+planted(1, 7)]
    ],
    [
        final(orchard__tree/1, [orchard__tree(7)]),
        ticks(1)
    ]).

fixture(nested_three_levels_keep_authored_arities,
    prog(
        [ col_type(orchard/1, orchard_id, int),
          col_type(orchard__tree/1, tree_id, int),
          rel_path_decl(orchard__tree/1, [orchard, tree]),
          col_type(orchard__tree__branch/1, branch_id, int),
          rel_path_decl(orchard__tree__branch/1,
                        [orchard, tree, branch]),
          col_type(grew/3, orchard_id, int),
          col_type(grew/3, tree_id, int),
          col_type(grew/3, branch_id, int)
        ],
        [ (orchard(OrchardId) <- grew(OrchardId, _, _)),
          (orchard__tree(TreeId) <- orchard(Oid), grew(Oid, TreeId, _)),
          (orchard__tree__branch(BranchId) <-
              orchard__tree(TreeId2),
              grew(_, TreeId2, BranchId)) ]),
    [],
    [
        [+grew(1, 7, 21)]
    ],
    [
        final(orchard__tree/1, [orchard__tree(7)]),
        final(orchard__tree__branch/1, [orchard__tree__branch(21)]),
        ticks(1)
    ]).

fixture(nested_child_and_an_option_column_keep_authored_shape,
    prog(
        [ col_type(orchard/1, orchard_id, int),
          col_type(orchard__tree/2, tree_id, int),
          col_type(orchard__tree/2, label, option(text)),
          rel_path_decl(orchard__tree/2, [orchard, tree]),
          col_type(planted/2, orchard_id, int),
          col_type(planted/2, tree_id, int),
          col_type(labelled/2, tree_id, int),
          col_type(labelled/2, state, text)
        ],
        [ (orchard(OrchardId) <- planted(OrchardId, _)),
          (orchard__tree(TreeId, 801) <-
              orchard(Oid), planted(Oid, TreeId)),
          (labelled(TreeId2, State) <-
              orchard__tree(TreeId2, LabelOption),
              '__opt_text_tag'(LabelOption, State)) ]),
    [],
    [
        [+'__opt_text_some'(801, "gala")],
        [+planted(1, 7)]
    ],
    [
        final(labelled/2, [labelled(7, some)]),
        ticks(2)
    ]).

fixture(nested_zero_column_child_is_one_flat_marker,
    prog(
        [ col_type(orchard/1, orchard_id, int),
          rel_path_decl(orchard__flag/0, [orchard, flag]),
          kind(orchard__flag/0, set),
          col_type(planted/2, orchard_id, int),
          col_type(planted/2, tree_id, int),
          col_type(flagged/1, orchard_id, int)
        ],
        [ (orchard(OrchardId) <- planted(OrchardId, _)),
          (orchard__flag <- orchard(Oid), planted(Oid, _)),
          (flagged(1) <- orchard__flag) ]),
    [],
    [
        [+planted(1, 7), +planted(1, 8), +planted(2, 9)]
    ],
    [
        final(orchard__flag/0, [orchard__flag]),
        final(flagged/1, [flagged(1)]),
        ticks(1)
    ]).
