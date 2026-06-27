# Decomposition objectives and cross-domain techniques

Date: 2026-06-26. Follow-up to `2026-06-26-engine-god-object-
decomposition.md`. The prior note found a clean field-coverage partition for
Engine (44 db_only / 48 stateful / 18 pure_fn) and validated it against a
cold-read 7-cluster decomposition. The validation surfaced the real gap: there
is no objective function. "Better" is preference, not measurement. This note
surveys the candidate objective functions and search algorithms from adjacent
fields, with citations hardened against the literature.

## The gap

Every analysis so far produces a candidate decomposition. None can prove
optimality. To pick "the next move" we need:

1. An **objective function** that scores a partition or an extraction.
2. A **search algorithm** that finds high-scoring candidates.
3. A **shape criterion** that says whether a candidate is well-formed.

Software engineering has well-known per-type metrics (Ca/Ce/instability, LCOM,
CBO) but lacks a standard partition-quality score. Adjacent fields have all
three.

## Candidate objective functions

### TurboMQ (Modularization Quality) - Mitchell-Mancoridis

The de facto standard objective for software partition quality. Per cluster C:

```
intra(C)  = call edges with both endpoints in C
inter(C)  = call edges with one endpoint in C, other outside
MF(C)     = intra(C) / (intra(C) + inter(C))   if intra > inter
          = 0                                    otherwise
MQ(P)     = sum over clusters C in P of MF(C)
```

Higher = tighter inside, looser outside. The Bunch tool hill-climbs over
partitions to maximize MQ. With MQ defined, "optimal next move" = argmax over
candidate extractions of `delta-MQ`.

- Reference: Mitchell, B.S. & Mancoridis, S. (2006), "On the Automatic
  Modularization of Software Systems using the Bunch Tool," *IEEE TSE* 32(3):
  193-208.
- Pros: standard, computable from edge counts.
- Cons: **call-edges only.** Field/state coupling is invisible. On Engine the
  call-edge graph blobbed (LCOM4=1, prior note section 4); the seam was in the
  field graph. A call-only MQ will rank the db_only extraction low because
  those 44 methods call `self.db`, not each other. Remedy: extend the edge set
  to include field-share edges, or run field-MQ alongside.

### Rent's rule - Landman-Russo

Empirical power law between external terminals (T) and internal blocks (g) of a
logic region: `T = t * g^p`. Realistic 2D circuits show `p in [0.5, 0.75]`;
chains give `p = 0`, cliques give `p = 1`, random placement gives `p = 1`.

Software mapping: terminals = pub methods on the extracted type; blocks = private
methods + fields. Fit `T = t * g^p` on a corpus of Rust crates (each existing
type contributes one (g, T) point). A candidate extraction is **Rent-consistent**
if its `(g, T)` falls on the line. Outliers signal bad shape: too few pub
methods for its size (sealed too tight, useless to the rest) or too many
(leaky, no isolation).

- Reference: Landman, B.S. & Russo, R.L. (1971), "On a Pin Versus Block
  Relationship For Partitions of Logic Graphs," *IEEE Trans. Computers* C-20(12):
  1469-1479. Interpretation: Christie & Stroobandt (2000), *IEEE TVLSI* 8(6):
  639-648. Software application: Mens, K. & Demeyer, S. (eds.) (2008),
  *Software Evolution*, Springer, ch. on modularity metrics.
- Pros: orthogonal to MQ (shape vs edge-count); cheap to compute.
- Cons: empirical, not theoretical; the corpus fit determines everything.

### Spectral / Fiedler vector - Fiedler; Shi-Malik

The Fiedler vector (eigenvector of the second-smallest eigenvalue of the graph
Laplacian `L = D - A`) gives a smooth, real-valued embedding of nodes; threshold
it to bisect, recurse to partition. Equivalent relaxation of min-cut.
Normalized-cuts (Shi-Malik) replaces `L` with the symmetric normalized
Laplacian `I - D^{-1/2} A D^{-1/2}`.

- Reference: Fiedler, M. (1973), "Algebraic Connectivity of Graphs,"
  *Czech. Math. J.* 23: 298-305. Shi, J. & Malik, J. (2000), "Normalized Cuts
  and Image Segmentation," *IEEE PAMI* 22(8).
- Pros: smooth min-cut; each node gets a coordinate, threshold anywhere; ranked
  seams, not binary.
- Cons: needs eigenvector computation, out of dl. Trivial in any BLAS / sklearn.

## Candidate search algorithms

The objective functions above are scoring rules. The algorithms below find
high-scoring partitions.

### Kernighan-Lin / Fiduccia-Mattheyses (KL/FM) min-cut bisection

Iterative improvement: start with a random bisection, swap vertex pairs to
reduce cross-edges until no improvement. FM is the linear-time-per-pass variant.
The basis of every modern VLSI partitioner.

- Reference: Kernighan, B.W. & Lin, S. (1970), "An efficient heuristic
  procedure for partitioning graphs," *Bell System Technical Journal* 49(2):
  291-307. Fiduccia, C.M. & Mattheyses, R.M. (1982), "A linear-time heuristic
  for improving network partitions," *DAC*.

### Multilevel hypergraph partitioning - hMetis (Karypis)

Hardware land's workhorse. Models the netlist as a **hypergraph**: one signal
net (e.g., an output driving N inputs) is one hyperedge, not N binary edges.
Multilevel: coarsen, partition the coarse graph, uncoarsen with refinement.
hMetis is the reference implementation; free for research.

Software mapping: a method calling N helpers is one hyperedge from caller to
the N callees. Treating it as N binary edges (as graph clustering does)
over-weights high-fan-out hubs. Hypergraph partitioning handles orchestration
hubs (Engine.tick calling 27 helpers) cleanly.

- Reference: Karypis, G., Aggarwal, R., Kumar, V., & Shekhar, S. (1999),
  "Multilevel hypergraph partitioning: applications in VLSI domain," *IEEE
  TVLSI* 7(1): 69-79.

### Bipartite spectral co-clustering - Dhillon

Spectral co-clustering on a bipartite graph simultaneously clusters both
partitions. Given the methods x fields incidence matrix, output is `(method_set,
field_set)` pairs: each cluster is a candidate struct with its state shape
included. Closes the layer-2 gap (cluster -> struct shape) that pure
method-method clustering cannot.

- Reference: Dhillon, I.S. (2001), "Co-clustering documents and words using
  bipartite spectral graph partitioning," *KDD*. See also Dhillon, Guan, Kulis
  (2007), "Weighted Graph Cuts without Eigenvectors: A Multilevel Approach,"
  *IEEE PAMI* 29(11).
- For Engine: SVD on a 110x16 matrix. One line in sklearn.

### Markov clustering (MCL) - van Dongen

Simulates flow on the graph: alternate "expansion" (matrix power of the
transition matrix) and "inflation" (raising entries to a power, renormalizing)
until convergence. Dense regions trap flow; sparse boundaries die. One
parameter (inflation `r`, controls granularity). No need to pick K.

- Reference: van Dongen, S.M. (2000), "Graph Clustering by Flow Simulation,"
  PhD thesis, University of Utrecht. (Note: Wikipedia's "Markov clustering"
  currently redirects to MCMC, which is a different family of algorithms. The
  canonical citation is van Dongen's thesis.)

### Girvan-Newman - edge-betweenness divisive clustering

Iteratively remove the edge with highest betweenness (number of shortest paths
traversing it), recompute, repeat. Produces a dendrogram; pick the cut that
maximizes modularity. Betweenness-based because inter-community edges carry
more shortest paths.

- Reference: Girvan, M. & Newman, M.E.J. (2002), "Community structure in social
  and biological networks," *PNAS* 99(12): 7821-7826.

### Force-directed placement - Hall; spring-electrical

Model edges as springs (attractive along edges, weakly repulsive globally),
solve for the equilibrium configuration. Hall's algorithm (1970) is the linear
algebra version: embed nodes using the Fiedler-vector-eigenvectors of the
Laplacian. Equivalent to spectral embedding followed by visual clustering.

- Reference: Hall, K.M. (1970), "An r-dimensional quadratic placement
  algorithm," *Management Science* 17(3): 219-229. Quinn-Breuer (1979) for the
  force-directed VLSI variant.

## Cross-domain pattern mining

### Frequent subgraph mining - gSpan

Detect recurring subgraph patterns across a graph corpus. Software mapping:
mine patterns across the type-graph of N codebases. A frequent pattern like
"N methods all touching one hub field" is the god-object anti-pattern
signature; its decomposition template (extract the hub as a collaborator)
transfers to every match found by subgraph isomorphism.

- Reference: Yan, X. & Han, J. (2002), "gSpan: Graph-Based Substructure Pattern
  Mining," *ICDM*. See also FFSM (Huan, Wang, Bandyopadhyay, Snow, Washington,
  Prins 2004) for an alternative.

### Consensus via isomorphism

Meta-technique: run N decomposition algorithms, intersect their agreements. If
field-coverage, hMetis, and Dhillon co-clustering all put the same 44 methods
in the db_only cluster, that cluster is high-confidence. Disagreements are the
diagnostic, not failure. Implementable via subgraph-isomorphism checks on the
cluster sets.

## Feasibility matrix

| Technique | Role | dl-native? | External tool needed |
|---|---|---|---|
| TurboMQ | scoring | partial (no `sum`?) | any BLAS / Python for sum |
| Rent's rule | shape check | yes (count pub/priv) | corpus fit |
| Fiedler / normalized cuts | scoring + bisect | no | sklearn / NetworkX |
| KL / FM | search | no | implementable in Rust, ~100 LoC |
| hMetis | search | no | hmetis binary |
| Dhillon co-clustering | search | no | sklearn `SpectralCoclustering` |
| MCL | search | no | NetworkX `markov_clustering` |
| Girvan-Newman | search | no | NetworkX |
| Force-directed | visualization + cluster | no | igraph / NetworkX |
| gSpan | anti-pattern mining | no | gSpan library |
| Consensus via isomorphism | meta | partial | custom |

The only dl-native scoring move is TurboMQ if `sum` exists in dl. Everything
else exports a relation (the per-cluster intra/inter table or the bipartite
incidence matrix) and runs externally. That matches the existing research
note's conclusion: dl's job is to produce the graph; the scoring can leave dl.

## Plan for Engine

Ranked by expected delta, given what the prior note already established:

1. **Bipartite method x field spectral co-clustering (Dhillon).** Cheap (SVD on
   110x16), tests whether the layer-2 gap is closeable, and outputs struct
   candidates directly. The input matrix is already implicit in the existing
   field-ref analysis. **Recommended next step.**
2. **TurboMQ over both edge kinds.** Score status quo (1 cluster), dl 3-way,
   cold-read 7-way, every-single-cluster-extracted. The argmax over single-
   cluster extractions of `delta-MQ` is the provably-best next move under the
   objective. If dl lacks `sum`, export the per-cluster `(intra, inter)` table
   and compute externally.
3. **hMetis on the call hypergraph.** Treat `Engine.foo` calling N helpers as
   one hyperedge. Run with k = 2..7 partitions. Compare to TurboMQ winner.
4. **Rent's-rule sanity check.** Fit `T = t * g^p` on Rust crates corpus;
   check every candidate extraction's `(g, T)` against the line. Outliers =
   bad shape.
5. **Consensus.** Intersect agreements across (1)-(4). The clusters every
   technique agrees on are high-confidence.

Step 1 is one command. Step 2 is one dl file plus a Python one-liner. The rest
is offline tooling.

## Limitations and known false friends

- **Modularity (Newman-Girvan) is not the same as TurboMQ.** Modularity
  compares edge density inside communities against a random null model; TurboMQ
  is a per-cluster intra/inter ratio. Both are valid; they optimize differently
  and can disagree.
- **MCL is not MCMC.** Wikipedia's redirect from "Markov clustering" to "Markov
  chain Monte Carlo" is wrong. MCL is van Dongen's deterministic flow
  simulation; MCMC is stochastic sampling. Do not substitute.
- **Frequent subgraph mining is anti-pattern detection, not decomposition.** It
  finds recurring shapes; the decomposition still comes from one of the
  algorithms above.
- **Spectral co-clustering assumes the incidence matrix is meaningful.** If
  fields are not consistently named (e.g., `Engine#db` vs `engine_db`), the
  bipartite graph fragments. RA's moniker shape (`Engine#<field>`) keeps it
  consistent for Rust; other languages need their own canonicalization.
