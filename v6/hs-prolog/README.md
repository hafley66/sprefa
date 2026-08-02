# v6/hs-prolog: Prolog and its ecosystem, answered in Haskell

Parked exploration, not on any critical path. Nothing here is imported by the
compiler, the runtime, or any gate. It exists to answer three questions that
kept coming up while v6's Prolog layer grew:

1. Does the Haskell ecosystem supply the graph algorithms SWI made us hand-write?
2. Which SWI powers does this repo actually depend on, and does Haskell have them?
3. If a Haskell component landed here, what would a working Haskell team expect
   to see in it?

A fourth question, whether `logict` can carry a real Prolog kernel, was still
running when this folder landed. Its lane is `lane/hs-interp`.

## Toolchain

GHC 9.14.1 and cabal 3.16.1.0, installed via brew. `swipl` 10.0.2 supplies the
differential answers. Every directory here is its own cabal project; the first
`cabal build` in each compiles dependencies and is exempt from the repo's
10-second law, graded runs are not.

## What is here

| directory | question | state |
|---|---|---|
| `graph/` | port all ten exports of `v6/prolog/0_graph.pl`, graded against SWI | 110/110 cells agree; timings WRONG, see below |
| `demand/` | which SWI powers the repo uses, and the Haskell answer per power | 24 probes pass; 5 features claimed without probes |
| `idioms/` | logging, tracing, errors, debugging, profiling as real projects do them | 10 projects cloned and cited; starter compiles and runs |

Each directory keeps the CONTRACT.md it was built against and the REPORT.md its
lane wrote. Reports are the lane's own words and carry the lane's own errors;
the corrections below are the coordinator's, verified independently.

## Known defects, verified by the coordinator

**`graph/app/Timing.hs` reports seconds labelled as milliseconds.** Line 20
divides `getCPUTime` (picoseconds) by `1e12`, which yields seconds, and line 23
appends `" ms"`. Every number in `graph/REPORT.md` under "Numbers" is 1000x too
small. Re-measured with the same module, `getMonotonicTime` alongside
`getCPUTime`, deepseq-forced:

    SCC     comps=1000   wall_ms=3.152   cpu_ms=3.078
    closure pairs=499500 wall_ms=151.681 cpu_ms=151.640

So the closure is about 172x faster than the Warshall composition, not 170,000x.
The qualitative finding holds: the sparse path tracks sparsity, the Warshall path
tracks vertex count.

**`graph/BUY.md` rejects fgl on a false premise.** It says fgl "does not expose
its SCC or TopSort modules." No modules by those names exist, but the functions
do, in `Data.Graph.Inductive.Query.DFS`. Compiled and run against fgl as
installed:

    DFS.scc     g  =>  [[1,2],[3]]
    DFS.topsort g  =>  [1,2,3]

The `containers` choice may still be right. The stated reason for eliminating
fgl is not. Note the second line: fgl's `topsort` returned an order for a cyclic
graph, so it shares the trap the report correctly caught in containers.

**`demand/probes` exits 0 even when a probe prints FAIL.** `app/Main.hs` runs
every probe group and never calls `exitFailure`. Confirmed by flipping a
condition in `Probe/Tabling.hs`: the run printed `FAIL tabling-fixpoint` and
still exited 0. Read the printed lines; the exit code proves nothing.

**`demand/probes/src/Probe/Sugar.hs:21` prints PASS unconditionally.** It is a
Template Haskell probe, so compiling is itself the evidence, but the line asserts
nothing at runtime.

## Verified by sabotage, not by reading

`graph/`'s grader is real. Making `graphClosure` reflexive, the exact silent
wrong answer `v6/prolog/0_graph.pl`'s header warns about, turns 12 cells red and
exits 1. Restoring returns 110/0 and exit 0.

`graph/fixtures/Golden.hs` is real SWI output, not invented. Dumped
`v6/prolog/0_graph.pl`'s answers for all 11 shapes directly from swipl and
compared: closure, components, cyclic components, topological order, and cycle
flag all match.

`idioms/`'s citations are real. Two picked at random and opened in the cloned
source: `rio/src/RIO/Prelude/RIO.hs:38` is the `RIO` newtype over `ReaderT env
IO`, and effectful's `Internal/Monad.hs:126` is `newtype Eff (es :: [Effect]) a`.
Commit shas in the report match the clones.

## The finding worth keeping

For a bench harness measuring Haskell, three numbers are commonly conflated and
`idioms/REPORT.md` separates them with one probe measured every way:

| quantity | source |
|---|---|
| total allocation volume over the run | `+RTS -s` `bytes allocated in the heap` |
| peak live heap at GC sample points | `max_bytes_used` |
| memory the RTS took from the OS | `max_mem_in_use_bytes` |
| process peak RSS | `/usr/bin/time -l` `maximum resident set size` |

`+RTS -t --machine-readable` emits all of the RTS side as a parseable assoc list,
which is what a harness wants.

The trap that matters: peak residency is not a property of the program. Measured
on one probe at n=600000, default RTS gives 18,366,456 bytes maximum residency
and `-A32m` gives 44,328. Same program, 400x apart. Any bench cell here has to
pin `-A` or the numbers are noise.

## The SWI question this started from

SWI-Prolog exports no strongly-connected-components predicate.
`library(ugraphs)` has 18 exports and none is SCC, and `pack_list('scc')` returns
no matching packages, which is why `v6/prolog/0_graph.pl` hand-writes Kosaraju.

SWI does contain Tarjan's algorithm, by Markus Triska, unexported inside
`library/clp/clpfd.pl` lines 5892-5962, where `all_distinct` and
`global_cardinality` use it. It operates on attributed variables and returns
nothing, leaving a `lowlink` attribute per vertex. The standalone version at
`https://www.metalevel.at/scc.pl` exists (4831 bytes, public domain) but imports
`atts`, `clpz`, and `dcgs`, which are Scryer Prolog libraries, so it does not
load in SWI.

Extracting that code into a callable SWI module is lane `lane/swi-scc`, running
when this folder landed.
