# 1. The six bodies

The [README](README.md) named three assumptions in the manual-paging frame and
the field each one hides. This chapter is the detail: for every field, the core
idea, the bounds that matter, the canonical papers, and how it lands on the actual
engine.

The running example is the same call graph as the rest of the book —
`main→run→parse→lex`, the `lex→run` cycle, `run→log` the sink, `helper` dead — so
the abstractions stay attached to something you can see.

```
   main ──▶ run ──▶ parse ──▶ lex
             │        ▲          │
             │        └──────────┘
             ▼
            log                     helper (isolated)
```

The question every field below answers a different way: **who reaches whom, and
where do the bytes for that answer live?**

---

## 1.1 Reachability and distance labeling

**The frame it breaks:** "the closure is a relation you store."

**The idea.** Building the full transitive closure is O(n²) tuples in the worst
case — for the example, every (caller, reachable-callee) pair. The labeling
literature never builds it. Instead it precomputes a small **label** L(v) on each
vertex such that a reachability query is answered by intersecting two labels:

```
   reaches(a, b)  ⟺  L_out(a) ∩ L_in(b) ≠ ∅          (2-hop cover)
```

Each vertex stores a handful of "hub" vertices it can reach (`L_out`) and a
handful that can reach it (`L_in`). A query touches two small sets, never the
closure. The art is choosing hubs so the labels stay small while still covering
every path.

**The variants and their bounds:**

| Method | Index size | Query time | Notes |
|---|---|---|---|
| Materialised closure (baseline) | O(n²) | O(1) | what the current frame forces |
| **2-hop cover** (Cohen et al. 2003) | O(n·√m) worst, far smaller in practice | O(\|L(a)\|+\|L(b)\|) | the foundational result |
| **PLL** — pruned landmark labeling (Akiba et al. 2013) | exact, scales to 10⁷–10⁸ edges | O(\|L(a)\|+\|L(b)\|) | the practical workhorse; prunes during a BFS-ordered build |
| **GRAIL** (Yıldırım et al. 2010) | O(n·k) interval labels | O(k), DFS fallback | cheap to build, approximate-prune then verify |
| **Tree-cover / chain decomposition** (Agrawal et al. 1989; Jagadish 1990) | O(n·width) | O(log n) | width = the graph's chain-decomposition width |

**On your engine.** This is the direct replacement for materialising `reaches`.
Build a PLL or GRAIL index over the typed call graph and reachability becomes a
two-label intersection that fits in RAM, instead of a closure relation you page.
It is the data-structure twin of the magic-sets idea: both refuse to materialise
the closure, one by indexing, one by re-ordering evaluation.

**Catch.** Labeling assumes a relatively static graph; updates need incremental
labeling (there is a literature, but it is the hard part). This is where it meets
your incremental-maintenance story and where the design work actually lives.

---

## 1.2 External-memory and cache-oblivious algorithms

**The frame it breaks:** "I hand-pick the page size and what is resident."

**The idea.** The RAM model charges O(1) per memory access and is simply false
across a hierarchy. The **I/O model** (Aggarwal & Vitter 1988) fixes this: charge
for block transfers, with M = memory size and B = block size, and count *I/Os*
instead of operations. Sorting is Θ((n/B)·log_{M/B}(n/B)) I/Os, not O(n log n)
operations.

The surprising part is **cache-oblivious** algorithms (Frigo, Leiserson, Prokop,
Ramachandran 1999): a single recursive, divide-and-conquer layout that is
I/O-optimal **without knowing M or B at all**. Because the recursion bottoms out
at every scale, it is simultaneously optimal for L1, L3, RAM, and disk. The
**van Emde Boas tree layout** and the cache-oblivious B-tree are the canonical
structures; funnelsort is the canonical sort.

**On your engine.** You are hand-tuning one boundary (RAM↔disk via SQLite pages).
A van-Emde-Boas-laid-out index, or a cache-oblivious B-tree for the fact store,
gives optimal transfer behaviour across *all* boundaries with no page-size knob to
pick. The lesson even if you never adopt the structures: analyse the fixpoint in
**I/Os, not operations**, because that is the cost you actually pay.

**Citations.** Aggarwal & Vitter 1988 (I/O model); Frigo et al. 1999
(cache-oblivious); Bender, Demaine & Farach-Colton 2000 (cache-oblivious B-tree).

---

## 1.3 Succinct and compressed data structures

**The frame it breaks:** "the representation is bulky, so paging is forced."

**The idea.** A succinct structure stores data in space close to its
information-theoretic minimum while still answering queries **without
decompressing**. The primitives are **rank** (how many 1s up to position i) and
**select** (where is the j-th 1), both O(1) on a bit-vector with o(n) overhead.
From those you build **wavelet trees**, the **FM-index** (Ferragina & Manzini
2000) for substring search, and — directly relevant — **k²-trees** (Brisaboa,
Ladra, Navarro 2009) that store a graph's adjacency matrix in a few bits per edge
while supporting neighbour and reverse-neighbour queries.

**The size shift.** A pointer-based graph at 133 MB is mostly pointers. A k²-tree
representation of the same adjacency can be a small multiple of the edge count in
*bits*, often an order of magnitude smaller, and you still walk it. If the whole
graph becomes resident, the paging problem does not get managed — it disappears.

**On your engine.** Two angles. First, represent the call-graph adjacency as a
k²-tree so the graph is resident. Second, note that chapter 6 already praised
Zoekt's compressed trigram postings: that is this field applied to text. The same
move applies to your fact tables.

**Citations.** Navarro, *Compact Data Structures* (2016, the textbook); Ferragina
& Manzini 2000 (FM-index); Brisaboa, Ladra & Navarro 2009 (k²-trees); Jacobson
1989 (rank/select origins).

---

## 1.4 Semi-streaming and linear graph sketches

**The frame it breaks:** "I must hold all the edges to answer graph questions."

**The idea.** The **semi-streaming model** (Feigenbaum et al. 2005) allows
O(n·polylog n) memory — proportional to *vertices*, not edges — with edges
arriving in a stream over one or a few passes. Connectivity, bipartiteness,
spanners, and matchings all fit.

The sharper tool is **linear sketches**. The **AGM sketch** (Ahn, Guha, McGregor
2012) maintains a small random linear measurement of the edge set that answers
connectivity, and because it is *linear* it supports **deletions** (turnstile
updates: ±edge). Alongside them sit **Count-Min** (Cormode & Muthukrishnan 2005)
for frequencies and **HyperLogLog** (Flajolet et al. 2007) for cardinality, both
tiny and resident.

**On your engine.** The deletion support is the hook. Your incremental story is
edits that add and remove call edges; an AGM-style sketch is updated by ±edge and
holds connectivity in vertex-space memory rather than a re-paged relation.
HyperLogLog answers "how many functions reach this sink" approximately, resident,
without the closure. These are approximate where your engine is exact, so they are
oracles and fast paths, not replacements for the ground truth.

**Citations.** Feigenbaum et al. 2005 (semi-streaming); Ahn, Guha & McGregor 2012
(linear graph sketches, dynamic connectivity); Cormode & Muthukrishnan 2005
(Count-Min); Flajolet et al. 2007 (HyperLogLog).

---

## 1.5 mmap and competitive paging

**The frame it breaks:** "residency is my job."

**The idea, two halves.** First, **mmap**: memory-map the store and let the OS page
cache be your buffer pool. LMDB (Howard Chu) is the thesis stated baldly — a
memory-mapped B+tree where there is no application-level cache to manage, the
working set is paged by the kernel, and you read as if it were all in RAM. Second,
**competitive paging theory**: if you *do* manage residency, there is a theory of
doing it optimally. LRU is k-competitive and the **marking** algorithms match it
(Sleator & Tarjan 1985); **Belady's** rule is the offline optimum (1966); the
**working-set** model (Denning 1968) describes what to keep. The combined message:
the kernel already runs a provably near-optimal policy, so hand-rolled residency
logic is usually re-deriving LRU worse.

**On your engine.** You are SQLite-welded, so you are already partly here — SQLite
pages through the OS. The *manual* part of the mental model is the self-imposed
piece. Leaning into mmap (or LMDB-style storage) hands residency back to a policy
with proofs behind it, and frees the design effort for shrinking the object
(1.1–1.3), which is where the real wins are.

**Citations.** Sleator & Tarjan 1985 (competitive paging, LRU); Belady 1966
(offline optimum); Denning 1968 (working set); LMDB design notes (Chu).

---

## 1.6 Differential and z-set algebra

**The frame it breaks:** "I materialise, then maintain."

**The idea.** Represent every relation as a **z-set**: a multiset with signed
multiplicities, where −1 means "this tuple was retracted." Then a query is a
function over z-sets, and its *incremental* version is the derivative: feed it the
**change** (the delta z-set) and it emits the change to the output, with state
proportional to the change rather than the data. **Differential Dataflow**
(McSherry et al. 2013) and its formalisation **DBSP** (Budiu et al. 2022/2023)
make this fully general for recursive queries — exactly the recursive `reaches`
shape.

**On your engine.** You already cite this lineage. The frame shift is to treat
delta-sized memory as the *default*, not an optimisation: the engine holds the
deltas flowing through the rules, and "where the bytes live" stops being about a
materialised closure and becomes about a stream of ±tuples. It is the algebraic
sibling of the labeling (1.1) and sketch (1.4) ideas — all three refuse to keep
the whole answer around.

**Citations.** McSherry, Murray, Isaacs & Isard 2013 (Differential Dataflow);
Budiu, McSherry, Ryzhyk, Tannen 2022/2023 (DBSP, incremental view maintenance).

---

## Reading list

Sorted by how cheaply you could try it against the current engine, cheapest first.

| Start | Paper / book | What you get | Bounds |
|---|---|---|---|
| **1** | Akiba, Iwata, Yoshida 2013 — Pruned Landmark Labeling | exact reachability oracle, replaces materialised `reaches` | index scales to 10⁷–10⁸ edges; query O(\|L(a)\|+\|L(b)\|) |
| **2** | Yıldırım, Chaoji, Zaki 2010 — GRAIL | cheap-to-build reachability index, good first experiment | index O(n·k); query O(k) with DFS fallback |
| **3** | Brisaboa, Ladra, Navarro 2009 — k²-trees | resident compressed adjacency for the call graph | a few bits/edge; O(1)-ish neighbour queries |
| **4** | Ahn, Guha, McGregor 2012 — linear graph sketches | connectivity under ±edge for the incremental path | O(n·polylog n) memory, turnstile updates |
| **5** | Budiu et al. 2022/2023 — DBSP | delta-as-default incremental maintenance, formalised | state proportional to change |
| **6** | Navarro 2016 — *Compact Data Structures* (book) | the whole succinct toolbox: rank/select → wavelet → FM-index | near information-theoretic space, queryable |
| **7** | Frigo et al. 1999 — Cache-Oblivious Algorithms | hierarchy-optimal layouts with no tuning knobs | I/O-optimal at every level simultaneously |
| **8** | Cohen et al. 2003 — 2-hop labels | the theory under PLL, for when you want the why | index O(n·√m) worst case |

The first three are weekend-sized experiments against the typed call graph. The
rest are the deeper reframes once an experiment shows the materialised closure was
the constraint all along.
