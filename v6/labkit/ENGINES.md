# labkit — every engine, painfully clear (and where "sqlite dd" is)

There are **two separate labs**. They do not share engines. The names collided
(`sqlite-reach` vs `SqlReconciler`) and that caused the fog.

```mermaid
flowchart TB
    GOAL["v6 engine = ONE single binary + SQLite<br/>reactive · temporal · durable"]:::goal

    GOAL --> L1
    GOAL --> L2

    subgraph L1["LAB A — RECONCILE  (salsa's job: which rels are stale?)"]
        direction LR
        SA["SalsaReconciler<br/>the salsa crate, resident"]:::ctl
        SQ["SqlReconciler<br/>SAME algorithm in SQLite"]:::ctl
        SA -.->|must match| SQ
    end

    subgraph L2["LAB B — REACH  (the product query: blast radius = all-pairs closure)"]
        direction LR
        RR["ram-reach<br/>BFS recompute"]:::naive
        SR["sqlite-reach<br/>CTE recompute"]:::naive
        CR["cascade-reach<br/>ON-DISK incremental<br/>= the 'sqlite dd' slot"]:::disk
        DD["dd-reach<br/>differential-dataflow<br/>RESIDENT yardstick"]:::bench
    end

    classDef goal fill:#12331f,stroke:#3fd88b,color:#d7ffe9,stroke-width:2px;
    classDef ctl fill:#2a1f3a,stroke:#b98cff,color:#ecdcff;
    classDef naive fill:#1e2a3a,stroke:#5aa9ff,color:#dbeafe;
    classDef disk fill:#3a2a12,stroke:#ffb454,color:#ffe9c7,stroke-width:2px;
    classDef bench fill:#2a2a2a,stroke:#777,color:#bbb,stroke-dasharray:5 5;
```

---

## LAB A — RECONCILE (salsa's job)

Two engines. Both maintain the digest of a dep DAG under edits. Same work, swappable.

| engine | where | what it is |
|---|---|---|
| **`SalsaReconciler`** | resident (RAM) | the real salsa crate does the red-green dirty-check |
| **`SqlReconciler`** | SQLite | the SAME dirty-check as a SQL sweep (revisions + digests in tables) |

**Proven:** identical recompute counts + identical answers at every scale; `SqlReconciler`
is 1.1–1.5× faster than the salsa crate. This is "salsa = reconciliation you can do in SQL."

This lab is NOT about reach. Ignore it when you're thinking about blast radius.

---

## LAB B — REACH (the product query: blast radius)

**All four compute the exact same thing** — the all-pairs transitive closure of the call
graph — and must produce byte-identical digests (they do, ✓). They differ only in *how*.

```mermaid
flowchart LR
    E["edit: one function's<br/>out-edges change"]:::e --> RR & SR & CR & DD
    RR["<b>ram-reach</b><br/>throw away the answer,<br/>BFS the WHOLE closure again<br/>in RAM"]:::naive
    SR["<b>sqlite-reach</b><br/>throw away the answer,<br/>recursive-CTE the WHOLE<br/>closure again, on disk"]:::naive
    CR["<b>cascade-reach</b><br/>keep the answer table,<br/>recompute only the<br/>AFFECTED sources, on disk"]:::disk
    DD["<b>dd-reach</b><br/>keep arrangements in RAM,<br/>maintain just the DELTA"]:::bench
    RR --> ANS["same closure digest ✓"]:::ans
    SR --> ANS
    CR --> ANS
    DD --> ANS

    classDef e fill:#241a08,stroke:#ffb454,color:#ffe9c7;
    classDef naive fill:#1e2a3a,stroke:#5aa9ff,color:#dbeafe;
    classDef disk fill:#3a2a12,stroke:#ffb454,color:#ffe9c7,stroke-width:2px;
    classDef bench fill:#2a2a2a,stroke:#777,color:#bbb,stroke-dasharray:5 5;
    classDef ans fill:#12331f,stroke:#3fd88b,color:#d7ffe9;
```

| # | engine | where | strategy | incremental? | measured (800 nodes) |
|---|---|---|---|---|---|
| 1 | **ram-reach** | RAM | BFS recompute from scratch each tick | no | 428 ms · slope ~1.7 |
| 2 | **sqlite-reach** | SQLite disk | recursive-CTE recompute from scratch | no | 6643 ms · slope 2.1 |
| 3 | **cascade-reach** | SQLite disk | keep `reach` table, recompute affected sources | *tries to be* | 8408 ms · slope 2.1 |
| 4 | **dd-reach** | RAM (resident) | differential arrangements, delta only | **yes, O(Δ)** | 102 ms · slope 1.1 |

- **1 & 2 are the naive baselines** (recompute everything) — the cost incrementality must beat.
- **4 (dd) is the yardstick** — it flattens the slope (102 ms vs 6643 ms) but is RESIDENT:
  memory slope 1.86 (1.7→75.7 MB), the path into the 5 GB gun. Benchmark only, does not ship.
- **3 (cascade-reach) is your "sqlite dd" slot** — the on-disk engine that should match dd's
  O(Δ) *and* survive the gun. **It does not yet.** See below.

---

## Where is "sqlite dd"? → `cascade-reach`, and it is NOT working yet

"sqlite dd" = an on-disk engine that maintains the closure **differentially** like dd, so it
gets dd's O(Δ) speed but keeps facts on disk (bounded RAM, survives the gun). The store crate
already proved this exists ("feldera in sqlite" = the semi-naive DRed cascade in `cascade.rs`).

`cascade-reach` is my first attempt at it, and the measurement says it **failed to be O(Δ)**:

- It is **correct** (digest ✓ every scale) and **bounded on disk** (mem slope 0.16). Good.
- But it is **as slow as full recompute** (8408 ms vs sqlite-reach 6643 ms, both slope 2.1). Bad.

**Why:** my version recomputes every *affected source* = every node that can reach the edited
node. On this graph the closure is dense (each node reaches ~375 others at 800 nodes), so the
"affected sources" set is almost the whole graph → it degenerates to full recompute.

The real "sqlite dd" must propagate the **delta** itself (the store's semi-naive
frontier→hits→next with retraction), not recompute sources. That is the next pass, and it is
the one you warned took several iterations.

```mermaid
flowchart LR
    subgraph NOW["cascade-reach TODAY (source-recompute)"]
        A1["edit at u"] --> A2["find ALL ancestors of u<br/>(≈ whole graph, dense)"] --> A3["recompute their<br/>full reach-sets"] --> A4["slow: slope 2.1 ✗"]
    end
    subgraph WANT["sqlite dd (delta cascade, store's cascade.rs)"]
        B1["edit at u (±edge)"] --> B2["propagate only the<br/>changed pairs (frontier)"] --> B3["retract lost pairs,<br/>assert new pairs"] --> B4["fast: O(Δ), on disk ✓"]
    end
    classDef x fill:#3a1a1a,stroke:#ff6b6b,color:#ffdede;
    classDef ok fill:#12331f,stroke:#3fd88b,color:#d7ffe9;
    class NOW x
    class WANT ok
```

---

## So, what I think you asked us to do

```mermaid
flowchart TB
    Q["your ask, as I understand it"]:::q
    Q --> A["1 · add dd as a real actor (yardstick)"]:::done
    Q --> B["2 · expose salsa's seam generically:<br/>default salsa, swap to sqlite"]:::done
    Q --> C["3 · optimize the sqlite side, find breaking points"]:::done
    Q --> D["4 · charts at different scales/knobs"]:::done
    Q --> E["5 · push sqlite to 2-3 GB, find where"]:::done
    Q --> F["6 · the on-disk 'sqlite dd' that matches dd<br/>(= a proper cascade-reach)"]:::todo

    A --> Ad["✓ dd-reach: O(Δ) slope 1.1, hits RAM wall"]:::note
    B --> Bd["✓ SalsaReconciler vs SqlReconciler:<br/>identical work, sqlite faster"]:::note
    C --> Cd["✓ SqlReconciler now beats salsa"]:::note
    D --> Dd["✓ charts.html open"]:::note
    E --> Ed["✓ 2 GB @ 80M rows · 3 GB @ 100M rows"]:::note
    F --> Fd["✗ cascade-reach degenerates on dense closure —<br/>needs the delta cascade (store's cascade.rs)"]:::todo

    classDef q fill:#12331f,stroke:#3fd88b,color:#d7ffe9,stroke-width:2px;
    classDef done fill:#1e2a3a,stroke:#5aa9ff,color:#dbeafe;
    classDef todo fill:#3a1a1a,stroke:#ff6b6b,color:#ffdede;
    classDef note fill:#131820,stroke:#232c39,color:#8a97a8;
```

If that last box (**6 · the real sqlite dd = delta cascade with retraction**) is the thing you
most wanted and I've been circling it — say so, and I port the store's `cascade.rs`
semi-naive DRed as `cascade-reach` v2, which is the one that should actually match dd on speed
while staying on disk.
