# L5 swi scc extract: REPORT

## Base proof

`git merge --ff-only a7108169` from the worktree root printed `Already up to date.`
Proceeded as instructed.

## The extraction

`v6/prolog/labs/scc_extract/scc.pl` is a standalone module exporting only
`scc_components/2`. The algorithm body, from `scc/2` through `v_in_stack//1`
plus the two `state//N` clauses, is copied verbatim from clpfd.pl lines 5892
through 5962. A whitespace-normalized diff of the extracted body against that
source range is a complete match: Triska's names and control flow are kept
unchanged, and the attribution comment naming him and clpfd.pl as the source
stays at the top of the file.

The extracted code depends only on SWI core: `get_attr/3`, `put_attr/3`,
`del_attr/2`, `phrase/3`, and DCG notation. Nothing clpfd-specific came across;
the module loads in a clean process with no clpfd loaded.

The wrapper (the only newly written code, plus the module/export header)
depends on `library(ugraphs)` and `library(pairs)`, both shipped.

Lines I changed and why (all in the wrapper, none in the extracted body):
- None of Triska's clauses were altered; the pass above proves the body is
  byte-for-byte (modulo whitespace) the clpfd original.

## The wrapper

The extracted `scc/2` works on unbound attributed VARIABLES, not atoms, and
returns nothing: it leaves each vertex carrying `index`, `lowlink`, and
(in_stack while on the DFS stack) attributes, and a callback supplies
successors. A caller therefore must: map atoms to fresh variables; supply a
callable successor closure that the algorithm invokes as `call(Succ, V, Tos)`
(a bare mapping list is not callable, and the closure must be module-qualified
so it resolves regardless of the caller); run it; then read the shared
`lowlink` back and group by that value.

Two rules make it work, and both are consequences of how SWI attributes behave:

1. Never unify a vertex variable. `put_attr(V, index, N)` makes SWI resolve a
   unify hook in a module named after the key, so unifying an attributed vertex
   with a non-variable raises "Unknown procedure: index:attr_unify_hook/2"
   (likewise `lowlink:...`, `in_stack:...`). Triska's code is safe inside clpfd
   because there it only ever sets and reads attributes and never binds the
   vertices. Any external driver must keep that discipline: use `==` (identity)
   for any lookup that could touch a vertex, never `=`. The wrapper does this in
   three places: the successor closure (`successor_lookup`/`succ_identical`),
   the atom-name lookup (`vertex_name`), and the map builder (`neighbours_by_var`).
   During the build phase, before any attributes exist, unification would not
   error, but it silently merges distinct vertex variables, so the identity scan
   is required there too for correctness, not just for safety.

2. The callback must return the SAME vertex variables it was given, not copies.
   My first two drafts used `findall` to build the closure map and the successor
   closure; `findall` copies its result, so the keys and successors became fresh
   variables that were never identity-equal to the vertices in `Vars`. The map
   must be built by plain recursion over the actual variables. This is the
   same hazard, on the other side of the interface.

What the coordinator's seeded attempt (`0_coordinator_failed_attempt.pl`) got
wrong, exactly:

- Its successor closure built the outgoing list with
  `findall(TVar, (member(N, Names), member(N-TVar, Map)), Tos)`. `findall`
  copies `TVar`, so the algorithm received fresh copy variables as successors,
  not the shared `Map` variables. Each copy was then traversed as its own
  unconnected vertex: it was not in `Map`, so the closure returned no name and
  an empty successor list, so no real vertex ever merged with anything. The four
  real vertices each kept their own lowlink (0,2,4,6) and, because each real
  vertex's discovery also spawned a copy that consumed an extra index slot via
  `index_plus_one`, indices advanced by two per vertex instead of one.
  Replacing that one `findall` with a recursive builder (`atoms_to_vars`) that
  reuses the same map variables produced the correct grouping, a=0 with b=0,
  c=2 with d=2, indices advancing one per vertex. `lowlink` is the right field;
  the "lowlink may not be the identifier" suspicion in the contract is ruled
  out. The successor closure was the fault, and the specific defect is the
  `findall` copy.

The wrapper is that structural cost: the attributed-variable identity discipline
(never unify, always `==`, no copying of vertex variables) plus a callable
closure plus a read-back-and-group step. My wrapper also had to get the ugraph
shape right (the diamond shape first came back `[[a],[b],[d],[c]]` until I
sorted the outer component list; the contract requires components ordered by
smallest member, matching `graph_components/2` which ends with `sort/2`).

## The grid

For each of the 11 shapes, `scc_components/2` (Tarjan), `graph_components/2`
(Kosaraju), and the Warshall oracle (only over cyclic components, the oracle's
contract) agree. Test run: `swipl -q -g run_tests -t halt
v6/prolog/labs/scc_extract/scc.test.pl`, exit 0.

| shape             | tarjan (scc_components)          | kosaraju (graph_components)      | warshall (cyclic only) |
|-------------------|----------------------------------|----------------------------------|------------------------|
| empty             | []                               | []                               | []                     |
| single_node       | [[lonely]]                       | [[lonely]]                       | []                     |
| chain             | [[a],[b],[c],[d]]                | [[a],[b],[c],[d]]                | []                     |
| self_loop         | [[a]]                            | [[a]]                            | [[a]]                  |
| mutual_pair       | [[a,b]]                          | [[a,b]]                          | [[a,b]]                |
| three_cycle       | [[a,b,c]]                        | [[a,b,c]]                        | [[a,b,c]]              |
| diamond           | [[a],[b],[c],[d]]                | [[a],[b],[c],[d]]                | []                     |
| two_cycles_joined | [[a,b],[c,d]]                    | [[a,b],[c,d]]                    | [[a,b],[c,d]]          |
| cycle_with_tail   | [[a],[b,c],[d]]                  | [[a],[b,c],[d]]                  | [[b,c]]                |
| flagship_shaped   | [[mid],[reach],[sink],[src]]     | [[mid],[reach],[sink],[src]]     | [[reach]]              |
| disconnected      | [[a],[b],[c,d]]                  | [[a],[b],[c,d]]                  | [[c,d]]                |

The plunit file asserts all three agree on every shape, plus that Tarjan's
result partitions the vertices exactly.

## Numbers

`statistics/2` with `walltime` (wall clock, ms) and `statistics(inferences,_)`.
Reference on the 1000-node chain: Kosaraju 11-12 ms / 145,088 inferences.

1000-node chain (acyclic, all singletons):

| impl        | wall (ms) | inferences   |
|-------------|-----------|--------------|
| Tarjan      | 172-174   | 3,053,925    |
| Kosaraju    | 12        | 145,088      |

1000-node cycle-plus-chords (one single component, cyclic-heavy):

| impl        | wall (ms) | inferences   |
|-------------|-----------|--------------|
| Tarjan      | 186-188   | 3,064,037    |
| Kosaraju    | 11        | 148,396      |

Inferences, which this repo's compile-speed gate pins, are around 20x higher
for the extracted Tarjan on both shapes. Every graded run finished well under
the 10 second ceiling.

## Verdict

1. Does the extracted Tarjan agree with our Kosaraju on all 11 shapes?
   Yes, on all 11, plus the Warshall oracle where it applies. Grid above.
   Additionally I fuzzed 360 random directed graphs (3 edge densities x 120
   sizes up to 120 nodes) and found no disagreement.

2. Is it faster? No, it is slower: roughly 15x on wall time and 20x on
   inferences on a 1000-node chain and on a cyclic-heavy 1000-node shape. The
   cost is the wrapper: identifying the current attributed vertex by identity is
   a linear scan in plain Prolog (O(V) per successor lookup, O(V^2) overall),
   and attributed-variable machinery is heavier than the assoc-backed Kosaraju
   in 0_graph.pl.

3. What did the caller have to do to drive attributed-variable code from
   outside, and is it worth it? The caller must map atoms to fresh variables,
   provide a module-qualified callable successor closure that returns the SAME
   variables (no copies), never unify any vertex variable (use `==`, never `=`),
   and read the shared `lowlink` back and group. Against 40 lines of hand
   Kosaraju on a plain ugraph, the wrapper is not worth it on this Prolog: it is
   an order of magnitude slower and demands a subtle discipline that the
   coordinator's first attempt already got wrong. If this repo needs SCCs on a
   ugraph, the 40 lines of Kosaraju are the better base unless the attributed
   interface can be fed a pre-indexed closure.

4. Any input where they disagree? I hunted via random fuzzing (above) and the
   11 seeded shapes; I found none. Both are textbook-correct strongly-connected
   component algorithms. I can report no input where they differ, but I cannot
   prove agreement for every graph, only that the searched space showed none.

## What I could not do

- I could not prove agreement for all possible graphs, only for the 11 seeded
  shapes plus 360 random graphs; proof is out of reach for a port like this.
- I could not make the extracted wrapper competitive with Kosaraju on time or
  inferences. The O(V^2) identity cost of matching an attributed vertex by
  `==` looks inherent to driving this interface in plain SWI-Prolog without a
  prebuilt index; I did not attempt a hash-indexed closure, so a faster wrapper
  may exist but is not demonstrated here.
