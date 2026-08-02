# Lane L5: pluck Tarjan out of clpfd and make it callable

## The finding this lane acts on

SWI exports no strongly-connected-components predicate. `library(ugraphs)` has
18 exports and none of them is SCC (verified: `top_sort/2 ugraph_layers/2
transitive_closure/2 transpose_ugraph/2 connect_ugraph/3 vertices/2 complement/2
add_edges/3 add_vertices/3 neighbors/3 compose/3 del_edges/3 del_vertices/3
vertices_edges_to_ugraph/3 edges/2 neighbours/3 reachable/3 ugraph_union/3`).
`pack_list('scc')` returns no matching packages. That is why
`v6/prolog/0_graph.pl` hand-writes Kosaraju.

But SWI DOES ship Tarjan's algorithm. It is in
`/opt/homebrew/Cellar/swi-prolog/10.0.2/lib/swipl/library/clp/clpfd.pl`,
lines 5892 through 5962, written by Markus Triska as a DCG that threads the
index, the stack, and a successor closure through its state term. `all_distinct`
calls it at line 5830, `global_cardinality` at line 6297. It is module-private,
undocumented, and unexported.

Your job: extract it into a standalone, callable SWI module, and prove it agrees
with our hand-written Kosaraju.

## Base

First action, from the worktree root:

    git merge --ff-only a7108169

Expected: `Already up to date.` Anything else: STOP, write REPORT.md, do not work
around it.

## Ownership

You own `v6/prolog/labs/scc_extract/**` and `REPORT.md` at the worktree root.
Nothing else. Do NOT edit `v6/prolog/0_graph.pl` or its test; you read them.
Other lanes are running in other worktrees.

## The exact source to extract

From clpfd.pl, these predicates, and nothing else:

    scc/2                    line 5892   the entry point
    scc//1                   5894
    vindex_defined//1        5899
    vindex_is_index//1       5901
    vlowlink_is_index//1     5905
    index_plus_one//0        5909
    s_push//1                5913
    vlowlink_min_lowlink//2  5917
    successors//2            5923
    scc_//1                  5925
    pop_stack_to//2          5937
    each_edge//2             5945
    state//1                 5958
    state//2                 5960
    v_in_stack//1            5962

Dependencies are `get_attr/3`, `put_attr/3`, `del_attr/2`, `phrase/3`, and DCG
notation, all SWI core. Nothing clpfd-specific should need to come with it. If
something does, say so in REPORT.md rather than dragging clpfd in.

Copy the code faithfully. This is a PORT, not a rewrite: keep the algorithm as
Triska wrote it. Keep the attribution comment naming him and clpfd.pl as the
source. If you change a line, the report says which line and why.

## The hard part, and a receipt so you do not start blind

The extracted code operates on unbound VARIABLES carrying attributes, not on
atoms. It returns nothing: it leaves each vertex's `lowlink` attribute set, and
vertices in one component are meant to share a value. A caller therefore has to
map atoms to fresh variables, supply a successor closure, call it, read the
attributes back, and group.

The coordinator tried exactly that and got a WRONG answer. The attempt is seeded
at `0_coordinator_failed_attempt.pl` in this directory. On the shape
`two_cycles_joined` (edges `a-b, b-a, b-c, c-d, d-c`), whose correct components
are `[[a,b],[c,d]]`, it printed:

    scc_ok
    a lowlink=0
    b lowlink=2
    c lowlink=4
    d lowlink=6

Four distinct lowlinks for four vertices, so no grouping at all, and the indices
advance by two rather than one. Two things worth suspecting and neither is
confirmed: the successor closure may be wrong, or `lowlink` may not be the field
that identifies a component after the run completes. Read `pop_stack_to//2` at
line 5937 closely; it is the predicate that writes the shared value.

Note also that the closure must be module-qualified when called from inside
clpfd (`user:my_successors(...)`), which the seeded file already does after a
first failure without it.

Diagnose it, do not paper over it. If the honest answer is that this code cannot
be driven from outside without changes, say that and show what change is needed.

## Deliverable

`v6/prolog/labs/scc_extract/scc.pl`, a standalone module exporting at minimum:

    scc_components(+Graph, -Components)

with EXACTLY the contract of `graph_components/2` in `v6/prolog/0_graph.pl`:
Graph is a ugraph, every vertex lands in exactly one component, a vertex on no
cycle is its own singleton, each component sorted, the list sorted, so components
come out ordered by smallest member.

Plus `v6/prolog/labs/scc_extract/scc.test.pl`, a plunit file.

## Grading: differential against both existing implementations

`v6/prolog/compile/test/0_graph.test.pl` holds 11 shapes and a Warshall
differential oracle. Use the same 11 shapes:

    empty, single_node, chain, self_loop, mutual_pair, three_cycle, diamond,
    two_cycles_joined, cycle_with_tail, flagship_shaped, disconnected

For each shape, three answers must agree: your extracted Tarjan,
`graph_components/2` (Kosaraju), and the Warshall oracle where its contract
applies (note it yields only the CYCLIC components, per the comment at the top of
the test file). Report the grid.

Run: `swipl -q -g run_tests -t halt v6/prolog/labs/scc_extract/scc.test.pl`

## Numbers

1000-node chain, and one cyclic-heavy shape of your choosing at the same size.
Time the extracted Tarjan against `graph_components/2`. Reference measurements
already taken on this machine: Kosaraju is 11-28 ms on the 1000-node chain, and
the Warshall composition is 26,166-30,155 ms.

Use `statistics/2` with `runtime` or `walltime`, and state which. Report the
inference count too (`statistics(inferences, _)`), because that is the number
this repo's compile-speed gate actually pins. Every graded run finishes under
10 seconds or you report the number as a finding.

## What the report must answer

1. Does the extracted Tarjan agree with our Kosaraju on all 11 shapes? Grid.
2. Is it faster, and by how much, on both time and inferences?
3. What did the caller have to do to drive attributed-variable code from
   outside, and is that wrapper cost worth it against 40 lines of hand Kosaraju?
4. Is there any input where they disagree? Hunt for one before saying no.

## Style laws (repo-wide, enforced)

- No em dashes anywhere.
- Banned words in prose AND identifiers: provenance, substrate, load-bearing,
  regime. Use source, base layer, critical, mode.
- Comments state only constraints the code cannot show. No change-log narrative,
  no dates. The attribution comment naming Triska and clpfd.pl STAYS; it is a
  source citation, not narrative.
- Descriptive variable names, never single-letter, in anything you write. The
  extracted code keeps Triska's names.

## REPORT.md format

    # L5 swi scc extract: REPORT
    ## Base proof
    ## The extraction
    <what came across, what changed and why, what it depends on>
    ## The wrapper
    <how atoms map to attributed variables and back, and what the coordinator's
     seeded attempt got wrong>
    ## The grid
    <11 shapes x 3 implementations>
    ## Numbers
    <time and inferences, Tarjan vs Kosaraju>
    ## Verdict
    <answers to the four questions above>
    ## What I could not do
