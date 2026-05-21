# 5. Where the bytes live

**The question:** you want a fact store that survives restart and stays small in
RAM even over a huge corpus. What are the layers, and what is the one discipline
that buys bounded memory?

## Three layers people all call "databases"

The zoo only makes sense once you see that the word "database" covers three
different things:

```
SERVER / DISTRIBUTED   a process you connect to over a socket, maybe a cluster
   Postgres, Neo4j, Materialize, CockroachDB
   ───────────────────────────────────────────────
EMBEDDED DATABASE      a library IN your process: data model + query language
   SQLite (rows/SQL)   DuckDB (columns/SQL)   Kuzu (graph/Cypher)   Cozo (datalog)
   ───────────────────────────────────────────────
STORAGE ENGINE / KV    "a brick": just put(key,bytes) / get(key) + durability
   RocksDB, LMDB (C/C++)        redb, sled (Rust)
```

A **brick** has no idea what a row is; it stores bytes by key. A **house** is a
brick plus a data model plus a query language, fused (SQLite, DuckDB) or sitting
on a brick you choose (Cozo runs on RocksDB or SQLite). A **server** wraps a
house in a process you talk to. Your `dl` engine is a thin house on top of
SQLite, which is itself a fused brick+house.

The four orthogonal questions that place any of them: where it runs (embedded /
server / distributed), data model (relational / graph / KV / column / document),
workload (OLTP small writes / OLAP big scans), and storage shape (row-store /
column-store / LSM-tree / B-tree).

## The one discipline: durable on disk, working set in RAM

Every system that stays small in memory over a large corpus obeys the same rule:

> The durable index lives on disk in a form the engine or the OS can page. Only
> the *working set* of the current operation is ever resident in RAM.

Three incarnations of the same idea:

```
SQLite     B-tree on disk + a page cache (and optional mmap). You touch a query's
           pages; the rest stays on disk. Cap residency: PRAGMA cache_size,
           PRAGMA mmap_size, sqlite3_hard_heap_limit64.

RocksDB    LSM-tree on disk + block cache. Writes batch into memtables, flush to
           sorted files; reads page from disk. Write-optimized, disk-resident.

Zoekt      mmap'd index shards. The OS pages in only the bytes read; the index
           can be many times RAM. Only small offset tables stay pinned.
```

The opposite design, "load the whole graph into an in-memory structure, then
answer queries," is what balloons RSS. It is fine until the corpus outgrows RAM,
then it falls over. The whole-corpus-in-RAM build is exactly where most code-graph
tools choke at medium scale.

## Storing a graph in SQL: four classic shapes

People solved "graphs in a relational store" decades ago (Celko). Four patterns,
and you already use two:

```
1. ADJACENCY LIST   edge(src, dst)                  traversal = recursive query
   simplest, general graphs, mutable.        ← your `calls`

2. CLOSURE TABLE    reach(src, dst)                 reads O(1), writes expensive
   precompute every reachable pair.          ← your `reaches`, materialized

3. NESTED SET       node(lft, rgt) from a DFS       fast subtree reads, hard writes
   trees only.

4. MATERIALIZED PATH node.path = '/main/run/lex'    descendants = path LIKE '...%'
   trees only, human-readable.
```

For a mutable directed graph (a call graph), adjacency list for the raw edges
plus a closure table for reachability is the textbook answer, and it is what you
landed on independently. Chapter 3's SCC-condensation is the refinement that
keeps the closure from being Theta(V^2) on disk: store the SCC partition and the
condensed closure instead of every concrete pair.

## Intuition

> "Database" is three layers: a brick (KV bytes), a house (model + query), a
> server (a process). Bounded RAM comes from one discipline shared by all the
> systems that scale: the durable index lives on disk and only the working set is
> resident. Storing a graph in SQL is the adjacency-list + closure-table pattern,
> condensed by SCC so the closure stays small.

## Exercises

1. Place each on the three-layer picture: SQLite, RocksDB, Cozo, Neo4j, redb.
2. Your `dl` keeps facts in SQLite and only the current file's parse in RAM. Over
   the Linux kernel that measured ~133 MB. Which "incarnation of the discipline"
   is that, and what would break it?
3. Why is "load the whole graph into a HashMap, then query" tempting and why does
   it fail at scale?
4. You store `reaches` as a closure table. On a graph with one big SCC of N
   nodes, how many rows is the naive closure? How does SCC condensation change it?

## In your engine

SQLite is your brick and house at once. `calls` is the adjacency list; `reaches`
is the materialized closure table. You already obey the discipline: facts on
disk, one file's parse resident, `PRAGMA`s available to cap the page cache. The
one refinement left is storing `reaches` SCC-condensed (Chapter 3) so a dense
cycle does not blow the closure up to Theta(V^2) rows on disk.

## Answers

1. SQLite = embedded house (fused brick). RocksDB = storage engine (brick). Cozo
   = embedded house on a brick (RocksDB or SQLite). Neo4j = server. redb = brick.
2. The SQLite incarnation (B-tree on disk + page cache). It would break if you
   ever materialized the whole graph in RAM to answer a query, or if a single
   query's working set (e.g. an un-condensed closure over one giant SCC) grew to
   the corpus size.
3. Tempting because in-RAM pointer-chasing is simple and fast to write. It fails
   because RSS then scales with total corpus size, so it works on your laptop
   demo and dies on a real repo.
4. A single SCC of N mutually-reaching nodes has N^2 reachable pairs in the naive
   closure. Condensed, that SCC is one super-node, so it contributes O(1) to the
   condensed closure and "X reaches Y within the SCC" is answered by "same SCC,"
   not stored pairs.
