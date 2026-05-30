# Relations

## in the engine

This is not a metaphor: dl runs exactly this. The code below is pulled straight from `v5/src/scc.rs` at build time (not pasted) — the real `Cond` struct the engine produces when it condenses the call graph. `comp` maps each node to its SCC; `cyclic` marks the looping ones; `cadj` is the condensed DAG that drives stratified evaluation.

graph: scc

code: ../src/scc.rs#L63-71

## now zoom out: the relations themselves form a graph

Everything above was the data graph: nodes are functions. There is a second graph one level up, where the nodes are the relations and the edges are rule dependencies. reaches depends on edge (the base rule) and on itself (the recursive rule). That self-loop is the recursion.

```prolog
% reaches is defined in terms of edge ... and reaches
reaches(X, Y) :- edge(X, Y).         % reaches  <- edge
reaches(X, Z) :- edge(X, Y), reaches(Y, Z).  % reaches <- reaches
```

```d2 relations
direction: right
edge.class: relation
reaches.class: relation
edge -> reaches: base rule
reaches -> reaches: recursive rule
```

anchor: reaches -> reaches
anchor: edge -> edge

## strata = SCCs of the relation graph

The same Tarjan/condensation, now applied to the relation graph, gives the strata: the order to evaluate relations in. A self-loop (reaches on reaches) means "recurse to fixpoint inside this stratum". A negated edge between relations that sat in a cycle would be the unstratifiable error the engine rejects.

graph: relations

```prolog
% predicate dependency graph -> strata
depends(reaches, edge).
depends(reaches, reaches).   % self-loop = recurse to fixpoint

% stratum(edge,    0).
% stratum(reaches, 1).
```

## the same graph, straight from SQLite

Every graph so far was drawn by hand in d2. This one is **a SQL query**. At build time the animator opens a SQLite database read-only, runs the SELECT, and turns the rows into the graph below — then the same pipeline auto-colors the cycle. It never imports the engine; it just reads the database file. The day `dl` lands its facts in SQLite, this fence points at the real kernel and the deck draws live code.

```sql-graph callgraph data/callgraph.sqlite
SELECT caller, callee FROM call_edge
```

## session note: what this deck is for

A frame needs **no code and no graph** — it can be a pure discussion note, so the deck doubles as a durable record of a session's thinking. Things parked here:

- **book/tree layout** — the deck is now a `src/deck/` tree of chapter files, mirrored in the FS. See [[01-reachability]] and [[02-cycles]].
- **the deck's own graph** — double-bracket links between slides form an import/export graph, rendered by the same d2 kit. Press `m` for the map.
- **no commits required** — edit a chapter file and it live-reloads.

> Authoring is just markdown: `## heading`, prose, optional ```` ```code ```` and ```` ```d2 ```` blocks. The FS tree is the table of contents.
