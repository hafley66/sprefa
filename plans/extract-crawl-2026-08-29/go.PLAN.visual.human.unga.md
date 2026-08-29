# go.PLAN: entrypoint crawl of typescript-go, in one story

## The crawl, drawn

```mermaid
flowchart TD
    A[5097 Go files<br>typescript-go] --> B[Per-file battery<br>5097 runs, all clean]
    B --> C[Per-package resolve<br>82 packages]
    C --> D[46055 resolved edges<br>113685 sites left hanging]
    D --> E[Entry crawl from main<br>+ compiler exports]
    E --> F[Parse plane:<br>201 of 18849 defs reachable]
    A --> G[scip-go index<br>./... on a scratch copy]
    G --> H[244055 scip_fn_edges]
    H --> I[scip plane:<br>22480 of 39244 reachable]
    D --> J[Kinks: cross-package,<br>interface dispatch, closures]
    H --> J
```

## What the numbers say

| where | parse plane | scip plane |
|---|---|---|
| defs | 18849 | 39244 |
| reachable from entrypoints | 201 | 22480 |
| reachable with test roots | 10118 | 31332 |

The parse arm can only see inside one package at a time, so a crawl from
`tsgo` main dies at depth 5. The compiler index reaches 57% of all defs
from the same roots.

## Top 5 kinks in plain words

1. The Go resolver never follows imports. Seven of ten call sites in the
   corpus have no edge because the callee lives in another package.
2. Interface method calls go nowhere. The parser gives interface methods
   no definition, so dispatch through an interface type is invisible.
3. Calls through function values (closures, callbacks) lose their edges.
   The site shrinks to one byte of source.
4. Builtins like `make` and `append` are names with no target.
5. The scip build path misfires: the wrong package pattern is passed to
   scip-go, and the time budget kills the run without saying anything.
