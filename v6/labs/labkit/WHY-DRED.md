# Why DRed — from scratch

## 0 · The job

Keep "which nodes are reachable from the root" **up to date as edges are added and
deleted**, without recomputing from scratch every time. That's it. `reach` is a derived
set; edges change; we maintain `reach` incrementally.

```mermaid
flowchart LR
    E["edges change<br/>(add / delete)"] --> M["maintain reach set<br/>incrementally"] --> A["answer: who is<br/>reachable from root?"]
    classDef n fill:#12331f,stroke:#3fd88b,color:#d7ffe9;
    class E,M,A n
```

---

## 1 · Adds are easy

Add an edge into a node that's already reachable → its target becomes reachable → keep
going forward. It only ever ADDS to the set. Nothing can become unreachable by adding an
edge. One forward sweep, done. (This never needed DRed.)

```mermaid
flowchart LR
    R((root)):::on --> A((A)):::on
    A -->|"add A→B"| B((B)):::new
    B -.->|propagate| C((C)):::new
    classDef on fill:#12331f,stroke:#3fd88b,color:#d7ffe9;
    classDef new fill:#241a08,stroke:#ffb454,color:#ffe9c7;
```

---

## 2 · Deletes are the hard problem

Delete an edge and some nodes **might** become unreachable — but only if they had no OTHER
path. The whole difficulty is: *how do you know if another path still exists?*

There are two ways to answer that. This is the entire fork in the road.

---

## 3 · Way A — Counting (weights). Cheap. **Wrong on cycles.**

Each node remembers **how many reachable parents support it**. Reachable ⇔ count > 0.
Delete an edge → decrement the target. Hits 0 → it dies → cascade the decrement forward.

On a tree/DAG this is perfect and fast (`reach_inc` in the lab):

```mermaid
flowchart LR
    R((root<br/>cnt 1)):::on --> A((A<br/>cnt 1)):::on --> B((B<br/>cnt 1)):::on
    R -->|"delete R→A"| A
    classDef on fill:#12331f,stroke:#3fd88b,color:#d7ffe9;
```
Delete R→A: A's count 1→0, dies; cascade → B 1→0, dies. Correct. 

### The catch: a cycle feeds itself

Now put a cycle in. Root reaches A; A→B→C→**A** is a loop.

```mermaid
flowchart LR
    R((root)):::on --> A((A)):::on
    A((A)) --> B((B)):::on
    B --> C((C)):::on
    C -->|"back-edge"| A
    classDef on fill:#12331f,stroke:#3fd88b,color:#d7ffe9;
```

Counts (how many reachable parents):

| node | supported by | count |
|---|---|---|
| A | root, **C** | 2 |
| B | A | 1 |
| C | B | 1 |

**Delete the only real edge, root→A.** A loses root, so A: 2 → 1. Still positive (C still
"supports" it). Cascade stops. Counting says **A=1, B=1, C=1 → all still reachable.**

But look at the picture: root is gone. A, B, C are a little island with no way in. **They
are unreachable — and counting says the opposite.** The cycle keeps its own counts alive
forever. This is not a tuning bug; counting is fundamentally blind to "the support is a
loop with no anchor."

```mermaid
flowchart LR
    R((root)):::dead -. "cut" .- A((A<br/>cnt 1 ✗)):::wrong
    A --> B((B<br/>cnt 1 ✗)):::wrong
    B --> C((C<br/>cnt 1 ✗)):::wrong
    C --> A
    classDef dead fill:#3a1a1a,stroke:#ff6b6b,color:#ffdede;
    classDef wrong fill:#3a1a1a,stroke:#ff6b6b,color:#ffdede;
```

---

## 4 · Way B — DRed (Delete and Rederive). Correct on cycles.

Don't trust counts. On delete, do two phases:

**Phase 1 — over-delete.** From the deleted edge's target, walk FORWARD and tentatively
mark the whole reachable cone as un-reached — cycle and all. Pessimistic: assume it might
all be dead.

**Phase 2 — rederive.** Bring back any node that STILL has an edge from a node that is
*genuinely* reached (a node OUTSIDE the cone, or the root). Propagate that forward. A dead
cycle has no such incoming edge, so it stays dead.

Same example, delete root→A:

```mermaid
flowchart TB
    subgraph P1["Phase 1 — over-delete the forward cone"]
        direction LR
        A1((A ?)):::susp --> B1((B ?)):::susp --> C1((C ?)):::susp --> A1
    end
    subgraph P2["Phase 2 — rederive: any in-edge from a REACHED node?"]
        direction LR
        A2((A)):::dead --> B2((B)):::dead --> C2((C)):::dead --> A2
        note["A's in-edges: root(dead), C(in cone)<br/>B's: A(in cone) · C's: B(in cone)<br/>NONE from a reached node → all stay dead ✓"]:::note
    end
    P1 --> P2
    classDef susp fill:#241a08,stroke:#ffb454,color:#ffe9c7;
    classDef dead fill:#1e3a24,stroke:#3fd88b,color:#d7ffe9;
    classDef note fill:#131820,stroke:#232c39,color:#8a97a8;
```

DRed gets it right: A, B, C correctly unreachable. Counting got it wrong. **That is the
only reason DRed exists** — it is the price of a correct answer when the graph has cycles.

If instead A also had a second real edge (say root→C), phase 2 would find C's in-edge from
root (reached) → C comes back → B and A come back through the cycle. DRed restores exactly
the nodes with a genuine anchored path. Correct either way.

---

## 5 · The decision — which one to use

```mermaid
flowchart TB
    Q{"can the graph<br/>have cycles?"}:::q
    Q -->|no — it's a DAG| CNT["COUNTING (reach_inc)<br/>cheap: work ∝ supports touched<br/>✓ correct on DAGs"]:::ok
    Q -->|yes| DR["DRed (reach_dred)<br/>correct on cycles<br/>cost ∝ the cone it over-deletes"]:::dred
    classDef q fill:#2a1f3a,stroke:#b98cff,color:#ecdcff,stroke-width:2px;
    classDef ok fill:#1e3a24,stroke:#3fd88b,color:#d7ffe9;
    classDef dred fill:#241a08,stroke:#ffb454,color:#ffe9c7;
```

**Why we care:** real code graphs are NOT DAGs. Call graphs have recursion; module graphs
have mutual imports; type graphs have mutually-recursive types. Cycles are normal. So for
those relations, counting would silently give wrong answers, and **DRed is the correct
engine.** For relations that are guaranteed acyclic, counting is cheaper and we'd use that.

That's the whole point of labbing both: not to ship two things, but to know **counting is
the fast path for DAGs, DRed is the correct path when cycles are possible** — and to
measure what DRed costs.

---

## 6 · What DRed costs (measured)

DRed over-deletes a whole cone, then rederives it. So its cost ∝ **the size of that cone**,
not the corpus:

| graph | root reaches | ms/round | note |
|---|---|---|---|
| sparse, small cone | ~0% | 9.5 ms | cheap — cone is tiny |
| dense SCC | 94% | 105 → 2044 ms as corpus grows | breaks — cone ≈ the whole reachable set |

- **Small cone → delta-proportional and cheap.**
- **One giant strongly-connected component → the cone is everything → O(reachable).** This
  is the "wavefront wall": deleting one edge in an SCC tentatively unreaches the whole SCC
  before rederiving it. No on-disk engine escapes this; only dd's resident arrangements do,
  and they pay in RAM.
- SQLite's own heap stayed **2.8 MB** at every scale — the on-disk win holds regardless.

---

## 7 · Does dd+salsa fix it, or is it universal? (both — different "its")

There are TWO problems in this doc. They have OPPOSITE answers.

```mermaid
flowchart TB
    subgraph P_A["Problem 1 · CYCLE CORRECTNESS (counting says A,B,C alive after the cut)"]
        A1["is it universal?"]:::q --> A2["NO — it's an ALGORITHM choice"]:::ok
        A2 --> A3["naive counting: WRONG on cycles<br/>DRed: correct · dd's iterate: correct BY CONSTRUCTION<br/>(signed diffs + consolidate compute the exact fixpoint delta)"]:::ok
    end
    subgraph P_B["Problem 2 · WAVEFRONT COST (cut an SCC's anchor → the whole SCC changes)"]
        B1["is it universal?"]:::q --> B2["YES — a LOWER BOUND no engine beats"]:::bad
        B2 --> B3["the answer genuinely changed for |SCC| nodes,<br/>so any engine that MATERIALIZES the answer must emit |SCC| updates.<br/>dd pays it. sqlite pays it. floor = |Δoutput|."]:::bad
    end
    classDef q fill:#2a1f3a,stroke:#b98cff,color:#ecdcff;
    classDef ok fill:#1e3a24,stroke:#3fd88b,color:#d7ffe9;
    classDef bad fill:#3a1a1a,stroke:#ff6b6b,color:#ffdede;
```

### Problem 1 (cycles) — dd fixes it by its algorithm, NOT because it's magic

dd's `iterate` doesn't naive-count. It maintains the fixpoint with **signed weights +
consolidation**, so when the anchor is cut it correctly drives the whole SCC to weight 0.
That's dd being a *correct* incremental algorithm, and **sqlite gets the identical
correctness by using DRed** (we measured: equiv ✓ on cyclic graphs). So this is not a
dd-only superpower — it's "use a correct algorithm." Counting is the wrong one; DRed and dd
are both right.

### Problem 2 (wavefront) — universal, hits dd and sqlite equally

If deleting one edge genuinely makes |SCC| nodes unreachable, the **output really changed
by |SCC|**. Any engine that keeps the full materialized answer must emit |SCC| changes.
That's a lower bound from the *question*, not a weakness of any engine:

| engine | work on the cut | where |
|---|---|---|
| dd | ≈ |Δoutput| = O(SCC) | **resident RAM** (arrangements) → grows → gun wall |
| sqlite DRed | O(cone) ≥ |Δoutput| (over-delete + rederive = pays the cone ~twice) | **on disk** → RAM bounded (2.8 MB) |

dd has the better constant (arrangements compute the exact delta; DRed over-approximates
with the cone). But **neither goes below |Δoutput|.** dd does not "fix" the wavefront cost —
it pays it in RAM instead of on disk.

### The only real escape — and it's salsa's property, not dd's

You dodge the wavefront cost only by **not materializing the whole answer**: demand-driven /
lazy evaluation. If you ask "is node X reachable?" instead of "give me the whole reachable
set," salsa computes just X and its dependency cone — the SCC's full delta is never paid
because you never asked for all of it. That's the one lever that beats the lower bound, and
it works by **changing the question** (pull one answer) rather than by a faster algorithm.

```mermaid
flowchart LR
    Q["cut an SCC anchor"]:::q
    Q --> M["want the WHOLE reachable set?"]:::plane
    M -->|yes| U["pay O(SCC) — UNIVERSAL<br/>dd in RAM · sqlite on disk"]:::bad
    M -->|"no, just 'is X reachable?'"| L["salsa demand-driven:<br/>compute only X's cone<br/>— dodges the floor"]:::ok
    classDef q fill:#2a1f3a,stroke:#b98cff,color:#ecdcff;
    classDef plane fill:#1e2a3a,stroke:#5aa9ff,color:#dbeafe;
    classDef bad fill:#3a1a1a,stroke:#ff6b6b,color:#ffdede;
    classDef ok fill:#1e3a24,stroke:#3fd88b,color:#d7ffe9;
```

### So, precisely

| the "it" | universal? | who fixes / pays |
|---|---|---|
| cycle correctness | **no** — algorithm choice | dd correct by construction; sqlite correct via DRed; counting is the wrong algo |
| wavefront cost when Δoutput is large | **yes** — lower bound = |Δoutput| | dd pays in RAM, sqlite on disk; neither beats it |
| escaping the wavefront cost | — | only by NOT materializing all of it → salsa's demand-driven pull |

---

## TL;DR

- Maintaining reach under **adds** = easy (propagate forward).
- Under **deletes** = the hard part.
- **Counting** the supports is cheap but a **cycle keeps itself alive after being cut** → wrong.
- **DRed** (over-delete the cone, then rederive the genuinely-anchored nodes) fixes exactly
  that → correct on any graph, at the cost of touching the whole cone.
- Code graphs have cycles, so **DRed is the correct engine**; counting is the fast path only
  when a relation is provably acyclic.
