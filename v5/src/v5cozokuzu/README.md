# v5cozokuzu

A throwaway comparison crate: the same call graph the `dl` examples build,
loaded into **Cozo** (embeddable Datalog + graph algorithms, Rust) and
**Kuzu** (embeddable property graph, Cypher), so you can see the same answer
from three engines and read how each phrases it.

It is its **own crate** nested under `v5/src/`. The v5 crate never declares it
as a module, so `cargo build` in v5 does not touch it. Each engine is behind a
feature flag because Kuzu compiles a C++ core and a build failure there must not
block the Cozo demo.

## Run

```
cd v5/src/v5cozokuzu
cargo run --features cozo-demo --bin cozo_demo
CMAKE_POLICY_VERSION_MINIMUM=3.5 cargo run --features kuzu-demo --bin kuzu_demo
```

## Build status on a current toolchain (rustc + rayon 1.12 + cmake 4.x), measured

Both demos hit real upstream dep-rot. The source is the deliverable ("how it
could be done"); running them today needs an older toolchain.

- **kuzu 0.11.3 / 0.10**: C++ core compiles, then the binary fails to LINK
  (`cxxbridge` symbols not found, macOS arm64). kuzu 0.6.1 linked and RAN once
  (output `run, parse, lex`), but is flaky on rebuild because cmake 4.x needs
  `CMAKE_POLICY_VERSION_MINIMUM=3.5` to get past kuzu's vendored re2, and that
  build does not reliably produce the bridge lib. Needs cmake 3.x to be solid.
- **cozo 0.7.6 and cozo-ce 0.7.13-alpha**: both depend on `graph_builder` 0.4.1,
  which does not compile against rayon 1.12. Build-blocking on both editions.
  Needs an older rayon pinned, or an upstream fix.

Captured kuzu 0.6.1 run (before the C++ build went flaky):

```
reach from main (Cypher variable-length path):
  run
  parse
  lex
```

## The same question, three surfaces

"What does `main` reach transitively?" → `run, parse, lex`

| engine | how reach is written |
| --- | --- |
| dl (yours) | `reaches(s,d) <- calls(s,d).`  `reaches(s,d) <- reaches(s,m), calls(m,d).` |
| Cozo | `reach[n] := *calls{caller:"main", callee:n}` then recursive rule |
| Kuzu | `MATCH (:Func{name:'main'})-[:Calls*]->(b) RETURN DISTINCT b.name` |

The point of difference: Cozo and Kuzu hand you graph **algorithms** (PageRank,
shortest path, components) as built-in operators whose output is a relation.
Your `dl` engine has the reactive front end they lack (file watching, content
hashing, mtime fast-path, incremental retraction). Cozo on its SQLite backend
is the closest off-the-shelf version of your derived+graph layer.
