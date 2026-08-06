% Module paths in FUNCTOR position. The text door parses `orchard.tree(X)` to
% rel_path/2; mangling needs a module id no phase mints yet, so it refuses.

fixture(module_path_in_head_refuses_by_name,
    prog(
        [ col_type(harvest/2, tree_id, int),
          col_type(harvest/2, picked, int)
        ],
        [ (rel_path([orchard, tree], [TreeId, Picked]) <- harvest(TreeId, Picked)) ]),
    [],
    [],
    [
        throws(unsupported_construct(module_path_unresolved([orchard, tree])))
    ]).

fixture(module_path_in_body_refuses_by_name,
    prog(
        [ col_type(ripe/1, tree_id, int)
        ],
        [ (ripe(TreeId) <- rel_path([orchard, fruit], [TreeId, _Picked])) ]),
    [],
    [],
    [
        throws(unsupported_construct(module_path_unresolved([orchard, fruit])))
    ]).

% A path deeper than two segments keeps every segment: the refusal payload is
% what h4's mangler will consume, so losing a level here would hide a defect.
fixture(module_path_three_segments_keeps_every_segment,
    prog(
        [ col_type(leaf/1, tree_id, int)
        ],
        [ (leaf(TreeId) <- rel_path([orchard, north, tree], [TreeId])) ]),
    [],
    [],
    [
        throws(unsupported_construct(
                   module_path_unresolved([orchard, north, tree])))
    ]).
