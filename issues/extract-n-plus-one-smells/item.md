---
created: 2026-09-08
updated: 2026-09-08
type: feature
reporter: fable
assignee: chris
status: open
priority: high
epic: extract-port-closeout
related: ['@extract-review-capabilities-in-extract']
labels:
- pkg:extract
- review-field-test
---

# extract: N+1 smells from df facts (loop-invariant scan, spread-accumulate, key-gated derivation)

## Description

## Description

Field test: could `extract` have flagged the N+1 that cost a 315 ms nav click in hafley-rxjs before a profiler did? Case file: `research/2026-09-08-n-plus-one-from-extract/README.md` (join script, before/after snapshots attached). Source commit pair: hafley-rxjs `2cd2717~1` → `2cd2717`, `packages/vitest-telemetry/src/report-app/adapter/navTree.ts`.

Three shapes were in the file:

| # | shape | before | after | fast tier today |
| --- | --- | --- | --- | --- |
| 1 | loop-invariant linear scan inside a loop: `for pid of processes` → `rows.filter(e => e.pid === pid)` | L97 | gone | `df_nest ⋈ site(start) ⋈ df_loop`, receiver ∉ {loop var, loop collection}, callee ∈ linear set: 1 hit, 0 false positives, 0.02 s |
| 2 | spread-accumulate inside a loop: `m.set(k, [...(m.get(k) ?? []), e])` | L43, L106, L126 | L126, L142 | cst `spread_element ⊂ df_loop.span` with `?? []` accumulator: 3 then 2 (survivors are short child lists, rule cannot tell) |
| 3 | derivation input threaded only as an id: `buildProcessNav(rows.$(), verdicts.$(), selected.$())`, `selectedTest` only feeds `===` | model L118 | gone | flow tier `arg_to_param` from `selected.$()` to the param (0.05 s); second hop has no fact |

Tier timings on the offending files: `--family df,call` 0.02 s; `--resolve --family call,flow` 0.05 s; `--ts-checker` 0.17 s (nothing rule-relevant); `--family scip` 2.0 s, index reused (nothing rule-relevant).

## Plan (types first)

Facts that make rule 1 and 2 native, no join outside extract:

```
df_nest      += callee: string, callee_path: string|null, receiver: {start,end}|null
df_nest      += loop_dependent: bool      // receiver expression reads loop.var (or a binding derived from it)
df_allocates    emit for ts: spread_element, array literal, object literal, template; owner = enclosing fn; loop = enclosing df_loop span|null
site.callee_path already carries `rows.filter`; keep, add `receiver_text` only if the receiver is not an identifier
```

Rule 1 as a smell row (`--family smell`, per @extract-review-capabilities-in-extract):

```
smell(kind = "loop_invariant_scan", call, loop) :-
  df_nest(call, loop, loop_dependent = false),
  df_nest.callee ∈ {filter, find, findIndex, some, every, includes, indexOf, map, reduce, forEach, flatMap, sort}.
```

Rule 2:

```
smell(kind = "spread_accumulate", alloc, loop) :-
  df_allocates(alloc, kind = spread, loop != null),
  alloc.text matches (X.get(_) ?? []) | (X ?? []) | X.concat(_).
```

Rule 3 needs one more family column: a per-param use classification. `df` emits `param_use(param, site, kind ∈ {compare, index, iterate, pass, call})`. Then:

```
smell(kind = "key_gates_derivation", arg, param) :-
  flow_edge(arg_to_param, from = <ident>.$(), to = param),
  ∀ use(param): kind = compare.
```

Pattern mode gap, separate from the families: `--ast-pattern` has no ancestor selector, so "call inside a loop" is inexpressible; `for ($V of $C) { $$$ }` without a capture is an error. Either an `--ast-inside ID=KIND` ancestor filter or documenting that structural nesting is the df family's job.

## Acceptance Criteria

- [ ] `extract --family df navTree.before.ts` emits `df_nest` rows carrying `callee`, `receiver`, `loop_dependent`; the L97 `rows.filter` row has `loop_dependent = false`, every `e.*` call inside `for (const e of rows)` has `true`
- [ ] `df_allocates` rows appear for ts spread/array/object/template with a `loop` span; navTree.before.ts yields 3 inside loops, navTree.after.ts yields 2
- [ ] `extract --family smell navTree.before.ts` prints `loop_invariant_scan` once (L97) and `spread_accumulate` three times; navTree.after.ts prints 0 and 2
- [ ] `param_use` rows exist for ts; `selectedTest` in navTree.before.ts has only `compare` uses
- [ ] `--ast-pattern` documents ancestor filtering or gains `--ast-inside`

## Tests Run

- `join.py` in the research dir reproduces the rule 1 and rule 2 counts above from today's `df,call` and `cst` output (before 1 / 3, after 0 / 2)

## Implementation Notes

- Join today is by `site.span.start == df_nest.call.start` because `site.span` is the callee token and `df_nest.call` is the whole call expression
- `df_allocates` is in the schema and emits 0 rows for ts on this file
- The profiler receipt these rules would have pre-empted: 179 ms self time in `buildProcessNav` per click over 279652 events, 315 ms → 15 ms after the fix
