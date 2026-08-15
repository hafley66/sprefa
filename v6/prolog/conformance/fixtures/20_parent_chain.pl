:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% A column typed option(<its own rel>) is the parent-chain shape. The
% companion split rel names the owner endpoint after the rel and the target
% endpoint after the column, so one CREATE TABLE carries two distinct atoms.
%
% rx lowering: expand over the parent join until none.
%   node$.pipe(mergeMap(child => parentOf$(child)), takeWhile(p => p !== none))
fixture(self_ref_option_chain_reads_through_the_companion,
    prog(
        [col_type(node/3, node_id, int),
         col_type(node/3, name, text),
         col_type(node/3, parent, option(node)),
         keyed(node/3, [1]),
         col_type(parent_name/2, child_name, text),
         col_type(parent_name/2, parent_name, text)],
        [(parent_name(ChildName, ParentName) <-
             node(ChildId, ChildName),
             node__parent(ChildId, ParentId),
             node(ParentId, ParentName))]),
    [],
    [
        [+node(1, "root"), +node(2, "mid"), +node(3, "leaf")],
        [+node__parent(2, 1)],
        [+node__parent(3, 2)],
        [-node__parent(3, 2)]
    ],
    [
        final(node/2, [node(1, "root"), node(2, "mid"), node(3, "leaf")]),
        final(node__parent/2, [node__parent(2, 1)]),
        final(parent_name/2, [parent_name("mid", "root")]),
        deltas(parent_name/2, [
            [],
            [+parent_name("mid", "root")],
            [+parent_name("leaf", "mid")],
            [-parent_name("leaf", "mid")]
        ]),
        ticks(4)
    ]).
