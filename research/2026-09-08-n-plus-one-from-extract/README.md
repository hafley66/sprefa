# N+1 in a loop: can `extract` find it before the profiler does?

Case: hafley-rxjs `packages/vitest-telemetry/src/report-app/adapter/navTree.ts`, commit `2cd2717~1` (before) vs `2cd2717` (after). A nav click took 315 ms; the profiler blamed `buildProcessNav`. Three shapes were in the file:

| # | shape | before | after |
| --- | --- | --- | --- |
| 1 | loop-invariant linear scan inside a loop: `for (pid of processes) rows.filter(e => e.pid === pid)` | L97 | gone |
| 2 | spread-accumulate inside a loop: `m.set(k, [...(m.get(k) ?? []), e])` | L43, L106, L126 | L126, L142 (children lists, short) |
| 3 | derivation input threaded only as an id: `buildProcessNav(rows.$(), verdicts.$(), selected.$())` where `selectedTest` only feeds `===` compares | model L118 | gone |

## Fast tier (`extract --family df,call FILE`, 0.02 s)

Facts used: `df_loop` (span, var, collection), `df_nest` (call span, loop span, collection), `site` (callee, callee_path, span = callee token only).

Rule 1, expressed as a join (`join.py`):

```
df_nest(call, loop) ⋈ site(start = call.start) ⋈ df_loop(loop)
where site.callee ∈ {filter, find, findIndex, some, every, includes, indexOf, map, reduce, forEach, flatMap, sort}
  and receiver(call) ∉ {loop.var, loop.collection, loop.var + "."…}
```

Result: before 1 hit (L97 `rows.filter` under `for … of processes`), after 0. No false positives in this file.

Rule 2 needs the cst family: `node(kind = spread_element) ⊂ df_loop.span` with text matching `(X.get(k) ?? [])` or `(X ?? [])`. Result: before 3, after 2 (the two survivors append one child per process; harmless, but the rule cannot tell).

What stopped it being a one-liner:

| gap | detail |
| --- | --- |
| `df_nest` has no callee and no receiver | the join to `site` is by start offset because `site.span` is the callee token, `df_nest.call` is the whole call |
| no loop-invariance fact | receiver vs loop var is string matching in python; a `df` fact "expression E does not depend on loop L's var" would make rule 1 native |
| no allocation facts for ts | `df_allocates` exists in the schema, 0 rows for this file; rule 2 had to read `spread_element` from the cst |
| `--ast-pattern` cannot say "inside a loop" | `for ($V of $C) { $$$ }` errors without a capture; there is no ancestor selector, so pattern mode finds every `$X.filter($$$)` (3 hits, 1 real) |
| `site.callee` for `rows.filter` is `filter`, `callee_path` is `rows.filter` | usable, but receivers like `Object.entries(x)` need the call text |

## Slow tiers

| tier | command | wall | what it added |
| --- | --- | --- | --- |
| resolve + flow (parse) | `extract --resolve --family call,flow model.ts navTree.ts` | 0.05 s | `flow_edge arg_to_param` from model L118 `selected.$()` to navTree L92 `selectedTest`: rule 3's first hop is expressible (a `.$()` read flowing into a param) |
| ts-checker | `extract --resolve --family call --ts-checker --project-root packages/vitest-telemetry …` | 0.17 s | 9 resolved edges, same `buildProcessNav` callers; nothing rule-relevant beyond the parse tier |
| scip | `extract --family scip packages/vitest-telemetry` | 2.0 s (index reused, 40 documents) | not needed for any of the three shapes |

Rule 3 second hop, "the param is only ever compared, never indexed or iterated", needs a per-param use classification (`read_kind ∈ {compare, index, iterate, pass}`) that no family emits today. With it: `arg_to_param(from = <signal>.$(), to = P) ∧ ∀ use(P): kind = compare` flags the input as a selection key that should not gate the derivation.

## Verdict

Fast tier catches shape 1 today with one 20-line join and shape 2 with the cst; both ran in under 0.05 s on the offending file. Native support means three facts: receiver + callee on `df_nest`, a loop-invariance bit per nested call, and `df_allocates` for ts spreads. Shape 3 is half-covered by the flow tier and needs param use kinds.
