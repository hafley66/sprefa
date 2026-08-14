:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% A rel with a list argument whose element type is the rel itself: the minted
% member value column is the self-referential node type, and the nested child
% node value in the list normalizes into its own node row plus the member row.
fixture(recursive_list_arg_parent_holds_child_node_values,
    prog(
        [type_decl(node, [col(name, text), col(children, list(node))]),
         col_type(node/2, name, text),
         col_type(node/2, children, list(node)),
         keyed(node/2, [1])],
        []),
    [],
    [
        [+node("root", 100),
         +'__gen__list_node_4205b0871c875897'(100),
         +'__gen__list_node_4205b0871c875897__member'(100, 0, obj([name-leaf, children-200]))]
    ],
    [
        final(node/2, [node("root", 100), node(leaf, 200)]),
        final('__gen__list_node_4205b0871c875897'/1,
              ['__gen__list_node_4205b0871c875897'(100)]),
        final('__gen__list_node_4205b0871c875897__member'/3,
              ['__gen__list_node_4205b0871c875897__member'(100, 0,
                  obj([children-200, name-leaf]))]),
        ticks(1)
    ]).
