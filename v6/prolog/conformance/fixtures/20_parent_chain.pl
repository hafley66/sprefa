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

% The explicit spelling of the same guard (rulings.pl acyclic_guard_spelling).
% Storage is the inner option, so this fixture and the bare one above run the
% same schedule over the same companion split rel.
%
% rx lowering: expand over the parent join until none.
fixture(acyclic_option_chain_matches_the_bare_spelling,
    prog(
        [col_type(node/3, node_id, int),
         col_type(node/3, name, text),
         col_type(node/3, parent, acyclic(option(node))),
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
        [+node__parent(3, 2)]
    ],
    [
        final(node/2, [node(1, "root"), node(2, "mid"), node(3, "leaf")]),
        final(node__parent/2, [node__parent(2, 1), node__parent(3, 2)]),
        final(parent_name/2,
              [parent_name("leaf", "mid"), parent_name("mid", "root")]),
        ticks(3)
    ]).

% acyclic wrapping an option of ANOTHER rel has no chain to walk, so it is
% named rather than silently dropped.
fixture(acyclic_over_another_rels_option_is_named,
    prog(
        [col_type(person/2, person_id, int),
         col_type(person/2, name, text),
         keyed(person/2, [1]),
         col_type(commit/2, commit_id, int),
         col_type(commit/2, reviewed_by, acyclic(option(person))),
         keyed(commit/2, [1])],
        []),
    [],
    [],
    [ throws(unsupported_construct(
               acyclic_not_a_self_option(commit/2, reviewed_by,
                                         option(person)))) ]).
