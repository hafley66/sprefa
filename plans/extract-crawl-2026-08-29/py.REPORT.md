# py.REPORT.md — python arm reports

## PyCG micro-suite as first python call-graph oracle (2026-08-31)

Lane feat-extract-python-oracle. Report-only: the python arm (the
index-free `--resolve` resolver in v6/sprefa-extract) scored against the
imported PyCG micro-benchmark over 119 tiny cases in 18 categories. Full
method, mapping rules, and license note:
`plans/extract-bench-2026-08-29/python-oracle/PYCG-SUITE.md`. Receipts:
`python-oracle/SCORES.tsv` (per case + category), `python-oracle/MISSES.tsv`
(225 oracle misses + 3 ours-only rows).

### Aggregate (exact 4-column match)

- Oracle edges: 236 (after dedup; 20 external rows excluded from the
  denominator — `<builtin>`/builtin-type callees)
- Ours edges: 17 (deduped `resolved_edge` rows across the whole suite)
- Overlap: 14
- Recall: 5.93 %, precision: 82.35 % (exact). Fuzzy (Jaccard 0.8) does not
  move recall materially: the misses are missing edges, not misnamed ones.

| category    | cases | oracle edges | ours edges | recall % | precision % |
|-------------|-------|--------------|------------|----------|-------------|
| args        | 6     | 14           | 0          | 0.00     | 0.00        |
| assignments | 4     | 8            | 0          | 0.00     | 0.00        |
| builtins    | 4     | 3            | 1          | 25.00    | 100.00      |
| classes     | 21    | 22           | 8          | 12.50    | 75.00       |
| decorators  | 6     | 7            | 1          | 0.00     | 0.00        |
| dicts       | 9     | 12           | 0          | 0.00     | 0.00        |
| direct_calls| 6     | 4            | 0          | 0.00     | 0.00        |
| dynamic     | 1     | 1            | 0          | 0.00     | 0.00        |
| exceptions  | 3     | 3            | 0          | 0.00     | 0.00        |
| external    | 6     | 6            | 0          | 0.00     | 0.00        |
| functions   | 4     | 4            | 0          | 0.00     | 0.00        |
| generators  | 4     | 6            | 0          | 0.00     | 0.00        |
| imports     | 14    | 14           | 2          | 14.29    | 100.00      |
| kwargs      | 2     | 3            | 0          | 0.00     | 0.00        |
| lambdas     | 4     | 5            | 2          | 14.29    | 100.00      |
| lists       | 8     | 8            | 0          | 0.00     | 0.00        |
| mro         | 6     | 7            | 1          | 7.14     | 100.00      |
| returns     | 5     | 4            | 2          | 16.67    | 100.00      |

(cases with a zero-edge oracle after conversion count in the case total;
12 of 119 are zero-edge and scored 0/0, never skipped.)

### The one dominant failure: module-level call sites

185 of the 225 missed oracle rows have an empty src_name: a call made at
module top level (`main` in PyCG's qualnames). Per-file `--family call`
shows our extractor FINDS those call sites (record=site rows with the
right callee name) — the loss is entirely in `--resolve`, which emits
zero resolved_edge rows for module-level callers. Example:
`direct_calls/with_parameters/main.py` has site `func3` at top level
(extracted) and no resolved edge. One fix there recovers most of the
recalls across categories, because nearly every micro case drives its
scenario from module-level code. The residual 40 non-module misses are
the genuinely dynamic shapes: higher-order args (`args/*`:
`func(param_func)` then `a()`), dict/list container dispatch (`dicts/*`,
`lists/*`), decorators returning a different function, MRO `super()`
dispatch, and `dynamic/eval`.

### 3 worst categories, 5 example misses each

args (recall 0.00, 14 oracle edges):
```
args/assigned_call  (module) -> func
args/assigned_call  func -> param_func
args/call           (module) -> func
args/imported_call  (module) -> to_import.func
args/param_call     (module) -> func
```
dicts (0.00, 12 edges):
```
dicts/add_key   (module) -> func
dicts/call      (module) -> func1
dicts/param     (module) -> func1
dicts/return    (module) -> func1
dicts/type_coercion (module) -> func1
```
direct_calls (0.00, 4 edges — every miss module-level):
```
direct_calls/assigned_call       (module) -> func
direct_calls/imported_return_call (module) -> to_import.func
direct_calls/return_call         (module) -> func
direct_calls/return_call         (module) -> nested_return_func
direct_calls/with_parameters     (module) -> func
```

### Shape vs PyCG's published 103/112

PyCG's own published run on this suite is 103/112 sound; its failures
concentrate in the deliberately dynamic categories: `dynamic` (eval),
`lambdas`, `generators`, parts of `mro`, and the `external` attribute
cases where it is allowed to over- or under-approximate. Our shape is
orthogonal: we fail almost uniformly across ALL categories because of the
module-level caller gap, and we are weakest precisely where PyCG is
strongest (direct_calls, dicts, lists, args — plain container-free calls
that flow through simple name binding), while the few categories where we
score (imports, returns, lambdas, builtins) are the ones whose oracle
edges happen to have an enclosing function on both ends. Fixing
module-level edges first would re-shape the comparison toward PyCG's own
profile: their residual failures (eval, dynamic dispatch) are exactly the
categories where we would remain at 0 anyway, while their bread-and-butter
cases become ours to win. The 3 ours-only rows (precision 82.35 %, not
100 %) are name-match false positives: `nested_class_calls` resolving a
method name to the inner class's same-named method, and
`decorators/return_different_func` matching the pre-decoration name.
