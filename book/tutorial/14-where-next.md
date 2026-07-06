# 14. Where to go next

> the examples browser, the book, the `std/` libs, the reference.

**Goal:** know where to look when a real question outruns this track.

You can now scan files, extract with regex and syntax trees, join into derived
relations, walk the built-in call and type graphs, write argmax two ways, ship
a `--check` rail with a suppression grammar, trace a value across function
boundaries, move a file with its imports, run effects under the daemon, splice
generated docs, and serve JSON-RPC. That is the whole loop. The rest is surface
area, and it is all discoverable from the binary.

## The examples browser

The `examples/` corpus is over a hundred programs, each solving one real problem.
It is embedded in the binary.

```sh
dl examples                      # list every example, name + one-line summary
dl examples taint dataflow       # search by meaning (nearest matches)
dl examples --show lint-imports  # print one program to stdout
dl examples --std                # the reusable std/ libraries
```

Run one straight from the binary without copying it to disk:

```sh
dl <(dl examples --show lint-imports) --root notes-app --no-daemon
```

Good next reads, by the thing you want:

- **Broken imports** as a lint: `dl examples --show lint-imports`.
- **A banned pattern** gate with `sg`: `dl examples --show ban`.
- **Call graph** variants: `dl examples --show callgraph`,
  `callgraph-ast`, `callgraph-resolved`.
- **A doc table** kept fresh by `gen`: `dl examples --show gen-type-table`.
- **Dataflow / taint**: `dl examples --show flow-interproc`,
  `dl examples --show taint`.

## The reusable libraries

`std/` holds `use`-able modules so you do not rebuild the call graph or the
dataflow union every time:

```dl
use "std/callgraph.dl".
use "std/flow.dl".
```

These resolve from disk if present, else from the embedded copy. `dl examples
--std` lists them.

## The reference

When you need the exact argument order of an operator or the columns of a
relation, read it, do not guess:

```sh
dl docs syntax       # every source op, body construct, and sink
dl docs functions    # every scalar function (split, replace, trim, ...)
dl docs relations    # every built-in relation and its columns
```

The same pages live under `docs/reference/` in the repo, generated from the
engine's own catalogs.

## The theory track

This track taught the *how*. The [book](../README.md) teaches the *why*: how the
fixpoint terminates, why cycles need SCC condensation, how incremental
maintenance retracts one file's facts, where the bytes live. Chapter 8 is the
deep version of lesson 5. Read `dl docs book` for the index, or `dl docs 2` for a
chapter.

## A few laws to carry

The gotchas this track met, in one place:

- **One relation, one rule kind.** A relation is written by source rules or
  derived rules, never both. Split and union (lesson 3).
- **Metavars are ALL-CAPS.** `$X` binds `X`; `$$$X` matches a whole list; a
  lower-case word in a pattern is literal code (lesson 2).
- **Closures are queried, not consumed unpinned.** For downstream joins, write
  the recursive rule (lesson 4).
- **Never hand-edit inside `BEGIN`/`END` markers.** Fix the generator (lesson 7).
- **Reserved names.** `repo`, `rev`, `file`, `ref`, and the `call_*` / `type_*` /
  `df_*` / module families are built in. Pick another name for your relations.

That is enough to write real programs. When one surprises you, the error message
usually names the fix, and `dl docs` has the rest.
