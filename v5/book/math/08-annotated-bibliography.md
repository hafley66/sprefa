# 8. Annotated bibliography

Every citation from chapters 1–7, grouped by topic, each with a one-line "why read
this." The chapter that uses it is in brackets. At the bottom is the Dover shelf: cheap
paperbacks that carry most of the prerequisites.

## Order theory & lattices

- Davey, B. A., Priestley, H. A. *Introduction to Lattices and Order.* Cambridge
  University Press, 1990. [[ch.1](01-order-and-lattices.md)] The standard textbook;
  chapters 1–2 are posets, lattices, completeness, exactly what the merge lattice needs.
- Birkhoff, G. *Lattice Theory.* AMS Colloquium Publications vol. 25, 1940.
  [[ch.1](01-order-and-lattices.md)] The origin text; dense but definitive on complete
  lattices.

## Fixpoints

- Tarski, A. *A Lattice-Theoretical Fixpoint Theorem and Its Applications.* Pacific J.
  Math. 5(2), 1955. [[ch.2](02-fixpoints.md)] The least/greatest fixpoint theorem on a
  complete lattice; the one theorem under every datalog engine.

## Datalog evaluation

- Bancilhon, F., Ramakrishnan, R. *An Amateur's Introduction to Recursive Query
  Processing Strategies.* SIGMOD 1986. [[ch.5](05-evaluation.md)] The survey that lays
  out naive vs semi-naive and the delta/frontier; read the semi-naive section.
- Abiteboul, S., Hull, R., Vianu, V. *Foundations of Databases* (the Alice book).
  Addison-Wesley, 1995. [[ch.5](05-evaluation.md)] The datalog chapters give the precise
  semi-naive operator and the stratified-evaluation theorem.
- Apt, K. R., Blair, H. A., Walker, A. *Towards a Theory of Declarative Knowledge.* 1988.
  [[ch.4](04-datalog-on-lattices.md)] The stratification result that lets negation and
  non-monotone aggregates layer above their inputs.
- Van Gelder, A., Ross, K. A., Schlipf, J. S. *The Well-Founded Semantics for General
  Logic Programs.* JACM 38(3), 1991. [[ch.4](04-datalog-on-lattices.md)] What to do when
  a program is *not* stratifiable (negation inside a recursive cycle).

## Semirings & lattices-as-values

- Green, T. J., Karvounarakis, G., Tannen, V. *Provenance Semirings.* PODS 2007.
  [[ch.3](03-semirings.md)] Unified annotated relations under one algebra; one
  evaluation, read off any property by choosing the semiring. Read §2–3.
- Abo Khamis, M., Ngo, H. Q., Pichler, R., Suciu, D., Wang, Y. R. *Convergence of Datalog
  over (Pre-)Semirings* (Datalog°). PODS 2022. [[ch.3](03-semirings.md)] Recursion over
  arbitrary semirings with one convergence theory; the recursive `path` rule
  parameterized.
- Madsen, M., Yee, M., Lhoták, O. *From Datalog to Flix: A Declarative Language for Fixed
  Points on Lattices.* PLDI 2016. [[ch.4](04-datalog-on-lattices.md)] The clean line:
  `min`/`max` (lattice `meet`/`join`) recurse, non-monotone aggregates stratify. Read
  §2–4.

## Graph algorithms

- Tarjan, R. *Depth-First Search and Linear Graph Algorithms.* SIAM J. Comput. 1(2),
  1972. [[ch.6](06-graph-cores.md), [parent §7.2](../07-the-fast-paths.md)] The original
  `index`/`low` SCC algorithm; one DFS, O(V+E).
- Fleischer, L., Hendrickson, B., Pınar, A. *On Identifying Strongly Connected Components
  in Parallel.* IPDPS 2000. [[ch.6](06-graph-cores.md)] The forward-backward
  divide-and-conquer SCC that replaces sequential DFS.
- Hong, S., Rodia, N. C., Olukotun, K. *On Fast Parallel Detection of Strongly Connected
  Components in Small-World Graphs.* SC 2013. [[ch.6](06-graph-cores.md)] Trim +
  coloring + forward-backward tuned for real-world graphs.
- Akiba, T., Iwata, Y., Yoshida, Y. *Fast Exact Shortest-Path Distance Queries on Large
  Networks by Pruned Landmark Labeling.* SIGMOD 2013. [[ch.6](06-graph-cores.md)] The
  2-hop label scheme: heavy build, O(label) queries, when queries outnumber edits.
- Dilworth, R. P. *A Decomposition Theorem for Partially Ordered Sets.* Annals of Math.,
  1950. [[parent §7.5](../07-the-fast-paths.md)] The min-chain-cover theorem under
  Soufflé's automatic index selection.

## Incremental maintenance

- Budiu, M., McSherry, F., Ryzhyk, L., Tannen, V. *DBSP: Automatic Incremental View
  Maintenance for Rich Query Languages.* VLDB 2023. [[ch.7](07-incremental.md)] The Z-set
  / derivative calculus; the cleanest "a relation is a stream of changes."
- McSherry, F., Murray, D. G., Isaacs, R., Isard, M. *Differential Dataflow.* CIDR 2013.
  [[ch.7](07-incremental.md)] The derivative idea over a partial order of timestamps; the
  arrangements that conflict with bounded RSS.
- Gupta, A., Mumick, I. S., Subrahmanian, V. S. *Maintaining Views Incrementally.* SIGMOD
  1993. [[ch.7](07-incremental.md)] DRed (delete-and-rederive) and counting; counting is
  sound on a DAG.
- Bender, M. A., Fineman, J. T., Gilbert, S., Tarjan, R. E. *A New Approach to Incremental
  Cycle Detection and Related Problems.* ACM TALG 12(2), 2015. [[ch.7](07-incremental.md)]
  Incremental SCC / cycle maintenance: update the condensation locally, not from scratch.

## Code-fact systems

- Subotić, P., Jordan, H., Guo, L., Scholz, B. *Automatic Index Selection for Large-Scale
  Datalog Computation.* VLDB 2018. [[parent §7.5](../07-the-fast-paths.md)] Computes the
  minimal set of composite ordered indexes via min-chain-cover; the full form of the
  engine's auto-index.
- Jordan, H., Scholz, B., Subotić, P. *Soufflé: On Synthesis of Program Analyzers.* CAV
  2016. [[parent §7.5](../07-the-fast-paths.md)] The datalog engine compiled to C++ for
  program analysis; the production reference for "datalog over code facts."

## The Dover shelf

Cheap paperbacks that carry the prerequisites. The first five are Dover reprints (a few
dollars each); the last two are the modern standards, not Dover but worth the price.

| Book                                                         | Carries                                          |
| ------------------------------------------------------------ | ------------------------------------------------ |
| Stoll, *Set Theory and Logic* (Dover, 1979)                  | relations, orders, bounds from first principles ([ch.1](01-order-and-lattices.md)) |
| Pinter, *A Book of Abstract Algebra* (Dover, 2010)           | groups, rings, the algebra under semirings ([ch.3](03-semirings.md)) |
| Trudeau, *Introduction to Graph Theory* (Dover, 1993)        | graphs, paths, connectivity, the vocabulary of [ch.6](06-graph-cores.md) |
| Smullyan, *First-Order Logic* (Dover, 1995)                  | the logic datalog rules are a fragment of ([ch.4](04-datalog-on-lattices.md), [ch.5](05-evaluation.md)) |
| Grätzer, *Lattice Theory: Foundation* (Birkhäuser)           | the deep dive past Davey & Priestley on lattices ([ch.1](01-order-and-lattices.md)) |
| Davey & Priestley, *Introduction to Lattices and Order* (CUP, 1990) | the working textbook for posets through fixpoints ([ch.1](01-order-and-lattices.md), [ch.2](02-fixpoints.md)) |
| Abiteboul, Hull, Vianu, *Foundations of Databases* (the Alice book, 1995) | the canonical datalog/evaluation reference ([ch.5](05-evaluation.md)) |

Read order if starting cold: Stoll for orders, Trudeau for graphs, then Davey &
Priestley chapters 1–2 for lattices and fixpoints, then the Alice book's datalog
chapters. The papers above are the depth past those.
