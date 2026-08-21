# Lane L2: does Haskell give the same graph powers as the SWI standard library

## Question this lane answers

`v6/prolog/0_graph.pl` is 196 lines that exist because `library(ugraphs)` gave
this repo four of the five things it needed and not the fifth. The repo's own
measured verdict is in `v6/prolog/ARCH.pl` under `task(prolog_graph_cleanup, ...)`:

- `transitive_closure/2` is STRICT positive-length reachability; `reachable/3`
  is REFLEXIVE and swapping them would have been a silent wrong answer.
- `top_sort/2` fails on cycles and self-loops, so it is the cycle detector too.
- SWI ships NO strongly-connected-components anywhere, and no pack does either
  (`pack_list('scc')` was empty), so SCC is hand-written Kosaraju, about 40 lines.
- Composing SCC out of `transitive_closure/2` is Warshall-cubic and tracks vertex
  count rather than sparsity: 27,082 ms on a 1000-node CHAIN against 27 ms.
- SWI tabling was priced for this and REJECTED on measurement (5 ms vs 1 ms).

Answer, with running code and numbers: what does Haskell's ecosystem give for
the same five jobs, and where does it land relative to that.

## Base

First action, from the worktree root:

    git merge --ff-only a7108169

Expected: `Already up to date.` Anything else: STOP, write REPORT.md, do not
work around it.

## Ownership

You own `labs/hs-prolog/graph/**` and `REPORT.md` at the worktree root. Nothing
else. Two sibling lanes are running; edits outside your subtree are a defect.

## Build-vs-buy comes FIRST, and it is a repo law

Before you write one line of graph code, write `labs/hs-prolog/graph/BUY.md`: a
candidate-by-candidate analysis. No one-line dismissals. At minimum:

| candidate | SCC | topsort | strict transitive closure | transpose | sparsity |
|---|---|---|---|---|---|
| `containers` Data.Graph | | | | | |
| `fgl` Data.Graph.Inductive | | | | | |
| `algebraic-graphs` | | | | | |
| hand-written over Data.Map | | | | | |

For each cell: the exact function name and its documented semantics, or "absent".
Two semantic traps to check by RUNNING, not by reading the docs:

1. Is the library's transitive closure reflexive or strict? The repo needs
   STRICT (a node appears in its own target set only when it sits on a cycle).
2. Does its topological sort FAIL, error, or silently return something on a
   cyclic graph? The repo needs failure, because that is its cycle detector.

Then a verdict paragraph: which candidate you build on and why.

## Deliverable

A cabal project at `labs/hs-prolog/graph/` porting all ten exports of
`v6/prolog/0_graph.pl`:

    graphFromEdges, graphFromEdgesWithVertices, graphNodes, graphClosure,
    graphReaches, graphComponents, graphCyclicComponents, graphComponentOf,
    graphTopologicalOrder, graphHasCycle

plus `BUY.md`, plus `REPORT.md` at the worktree root.

## Grading: the differential, against SWI itself

`v6/prolog/compile/test/0_graph.test.pl` already holds 11 shapes and 15 plunit
tests, including a differential Warshall oracle. Use the SAME shapes:

    empty, single_node, chain, self_loop, mutual_pair, three_cycle, diamond,
    two_cycles_joined, cycle_with_tail, flagship_shaped, disconnected

Procedure:

1. Run the SWI side and capture its answers as oracle. From `v6/prolog`:
   `swipl -q -g 'run_tests' -t halt compile/test/0_graph.test.pl` for the pass
   count, and a small `-g` script that prints each predicate's answer per shape
   for the actual values.
2. Write those values into a fixture file in your subtree.
3. Your Haskell grader prints `PASS <shape>/<predicate>` or `fail` per cell and
   exits nonzero on any fail.
4. Ordering counts. `graph_components/2` returns components sorted, each
   component sorted, so components come out ordered by smallest member. Match
   that exactly, or report where you do not.

## Numbers, required

The repo's verdict rests on measurements. Reproduce the shape of them:

- 1000-node CHAIN: time your SCC and your closure. The SWI numbers to compare
  against are Kosaraju 27 ms and Warshall composition 27,082 ms.
- Say whether your chosen Haskell path tracks sparsity or vertex count.
- Every graded run must finish under 10 seconds (repo law). The first
  `cabal build` is exempt. Report any run that breaks 10 s as a finding.

## No-cheat rules

1. Do not copy an existing Haskell port of ugraphs, and do not transcribe
   `0_graph.pl`'s Kosaraju line by line into Haskell if a library already ships
   SCC. The question is what the ECOSYSTEM gives, so using `containers`'
   `stronglyConnComp` is the right answer if it is the right answer; hiding a
   hand port behind a library name is not.
2. Allowed dependencies: anything on Hackage, provided BUY.md justifies it.
3. Read `v6/prolog/0_graph.pl` and its test for SEMANTICS. That is the spec, not
   cheating. Copying its algorithm when a library supersedes it is the failure
   mode to avoid.

## Toolchain

`ghc` 9.14.1 and `cabal` 3.16.1.0 are at `/opt/homebrew/bin`, Hackage index is
fresh, and `logict`, `containers`, `fgl`, `algebraic-graphs`, `mtl` are all
verified to build together on this machine. `swipl` is at `/opt/homebrew/bin/swipl`.

## Style laws (repo-wide, enforced)

- No em dashes anywhere.
- Banned words in prose AND identifiers: provenance, substrate, load-bearing,
  regime. Use source, base layer, critical, mode.
- Comments state only constraints the code cannot show. No change-log narrative,
  no dates, no restating the next line.
- Type signature first, pseudo-code comment under it, body after.

## REPORT.md format

    # L2 hs graph: REPORT
    ## Base proof
    <git merge --ff-only output, verbatim>
    ## Buy verdict
    <one paragraph, pointing at BUY.md>
    ## The differential
    <the 11 x 10 grid result, plus the grader output verbatim>
    ## Numbers
    <1000-node chain timings, both algorithms, against the SWI numbers>
    ## Same powers? the five jobs
    <closure strictness | topsort-as-cycle-detector | SCC | transpose | neighbours>
    <for each: SWI answer, Haskell answer, verdict>
    ## What I could not do
    <every gap, named>
