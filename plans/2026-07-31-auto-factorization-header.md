# Auto-factorization lab — header (planner-seeded, opus, queued behind atlas-variants merge)

User directive 2026-07-31: "do such good enough graph code dataflow
everything analysis to come up with a way to auto toposort
search/measure how clustered specific files/functions/folders/types/
paths are common to each other for auto factorization, and including
fake hops like what if we made an interface cut here."

Extends the parked auto-architect umbrella (docs/vision-auto-architect.md)
with the piece it lacked: a MEASURED clustering plane over real
extracted facts, plus counterfactual cuts graded as data.

## Fact base

The dataflow atlas (v6/dl/fixtures/dataflow-atlas.dl6): 421 nodes /
809 edges, 4 language planes, resides/calls/imports/bridge edge
families. The lab READS these facts (or regenerates them); it does not
re-extract.

## Questions the lab must grade (each with receipts, in dl6 where expressible)

1. **Toposort/strata depth as a rel**: derive per-node depth over the
   acyclic call/import graph. Already proven shape (the atlas computed
   longest path recursively). Grade: depth rel byte-stable, agrees with
   `dot -Tplain` ranks on a sample.
2. **Cohesion/coupling per grouping axis**: for each axis
   (file, folder, language plane, prolog module, TS type via sig facts):
   internal-edge count vs cross-edge count per group, the classic
   modularity ratio. Grade: the numbers for the CURRENT partition,
   stated per axis.
3. **Community detection = BUY RESEARCH FIRST** (standing law): price
   real implementations before writing any — candidates to research:
   graphology-communities-louvain (TS), Leiden reference impls,
   sqlite-expressible label propagation (recursive CTE), python
   networkx/igraph as offline referee. The referee matters more than
   the in-language impl: an offline lib grades whatever we lower.
4. **The counterfactual cut ("fake hops")**: a proposed interface cut =
   a HYPOTHETICAL node splicing a set of cross-cluster edges
   (callers -> IFACE -> callees). As rows: cut(name, edge_set). Derive
   the metric delta: modularity before/after, max cluster size
   before/after, new toposort depth. Zero engine changes expected —
   this is rel algebra over the edge facts with a union. If a needed
   shape is refused, record the named refusal.
5. **Auto-search**: rank candidate cuts by metric improvement. Start
   exhaustive over small candidate sets (every folder boundary, every
   bridge edge), not heuristic search — smallest correct first.
6. **Auto-numbering** (user 2026-07-31: "auto cluster and number our
   files and folder systems"): derive the numeric file prefix
   (0_types, 1_binds, ...) from toposort depth per file within each
   folder, and folder ordering from cluster depth. OUTPUT = a proposed
   rename table (current path -> numbered path) per package, NOT
   applied renames (renames touch every import; applying is its own
   arc after user review). Grade: where the derived numbers DISAGREE
   with the existing hand-assigned prefixes (v6/tsv2/src, v6/dl/src,
   v6/prolog compile files), each disagreement is either a
   hand-numbering error or a metric error — classify every one.
   Ties (same depth) need a stated tiebreak (edge count, then name).

## Grading references (the answer key problem)

- The prolog packaging research lane's partition candidates (in
  plans/2026-07-31-prolog-packaging-research.md when landed) are
  HUMAN-PRICED partitions of the same code: the auto-factorizer run
  over the prolog plane should rank a similar cut highly, and where it
  disagrees, the disagreement is the finding.
- The user's restart carve (extract / store / vscode / prolog /
  goldens) is a second reference partition at repo granularity.

## Named slots (ambiguities the lab may hit)

- SLOT-METRIC: modularity vs conductance vs plain ratio — which number
  is the ranking key.
- SLOT-CUT-GRANULARITY: edge-set cuts vs node-split cuts.
- SLOT-TYPE-AXIS: TS types come from sig facts; prolog has no types —
  what the type axis means per plane.
- SLOT-SCALE: 421 nodes is toy; the v5 corpus graph is the real target.
  State what breaks at 10k nodes.

## Protocol

Lab protocol applies (worktree, opus per the labs-on-opus directive,
lab dies on landing, durable output distills to plans/ verdict +
fixtures). Fences at dispatch time will name the atlas files read-only
and exclude the two host-seam lanes' files.
