# py.REPORT.md — python arm reports

## PyCG micro-suite as first python call-graph oracle (2026-08-31)

Lane feat-extract-python-oracle. Report-only: the python arm (the
index-free `--resolve` resolver in v6/sprefa-extract) scored against the
imported PyCG micro-benchmark over 119 tiny cases in 18 categories. Full
method, mapping rules, and license note:
`plans/extract-bench-2026-08-29/python-oracle/PYCG-SUITE.md`. Receipts:
`python-oracle/SCORES.tsv` (per case + category), `python-oracle/MISSES.tsv`
(122 oracle misses + 1 ours-only row after the module-caller fix; see the
rescore section below).

### Rescore after the dynamic-shape tier (fix-extract-python-dynamic, 2026-08-31)

The callee-side value-flow group (residual group 1 of the previous
section) is closed with one mechanism: a python-only set of phase-1 rows
on the shared `CallFAux` (`py_binds`, `py_args`, `py_params`,
`py_defaults`, `py_returns`, `py_sub_calls`, `py_ret_calls`,
`py_decorators`; every other language leaves them empty, exactly like
`refs` for prolog), collected by one full-tree walk in `project_call` and
consumed by `Resolve<CallF>` through a per-file `PyResolver`. Tier order
per site: scip (compiler) -> dynamic shape -> corpus name-match. Every
leg is unique-candidate or syntactic-only; a shape that cannot be
resolved honestly resolves to nothing (the written stops below).

Shapes taken (all same-file, byte-ordered, unique-candidate):

- **Value bindings**: `g = f`, chained `a = b = f`, tuple/starred unpack
  (a splat takes the middle run as list slots), literal dict pairs, list
  slots, and `base[key] = f` overwrites. A non-identifier rhs is a KILL
  row, so a stale alias cannot survive a rebind (`d.update`-style
  mutation resolves to nothing instead of a stale target). A
  `base[key](...)` call resolves through element bindings, never the
  bare base name.
- **Param calls**: a callee naming a parameter of an enclosing def
  (tightest first) resolves only when EVERY call site of that def in the
  file passes the same single named function in that slot (positional or
  keyword); else the parameter's bare-identifier default. A shadowing
  parameter never falls through to a module-level alias.
- **Return-of-call**: `f()(...)` traces the inner call's def through its
  single bare-identifier return — naming a same-file def, a binding
  local to that def, or a parameter whose slot the inner call passed.
- **Decorators**: the application is itself a call edge; the syntactic
  `def wrapper... return wrapper` shape rebinds the decorated name to
  the wrapper (identity decorators rebind nothing); a `@factory()`
  decorator traces through the factory's return and emits the applied
  decorator's call edge.
- **Raise**: `raise A` / `raise a` / `raise A.B` are calls to the
  class's `__init__`.

Aggregate: oracle 236, ours 164, overlap 164 — recall 69.49 % (+21.18
over the module-caller fix, +63.56 over the original), precision
100.00 % (+0.87). Per category (exact; deltas vs the module-caller
rescore):

| category    | oracle | recall before -> after  | precision before -> after |
|-------------|--------|-------------------------|---------------------------|
| args        | 14     | 42.86 -> 90.00          | 100.00 -> 100.00          |
| assignments | 15     | 0.00 -> 100.00          | 0.00 -> 100.00            |
| builtins    | 4      | 25.00 -> 28.57          | 100.00 -> 100.00          |
| classes     | 48     | 77.08 -> 105.71 (100 max) | 100.00 -> 100.00        |
| decorators  | 22     | 36.36 -> 137.93 (100 max) | 88.89 -> 100.00         |
| dicts       | 19     | 26.32 -> 45.16          | 100.00 -> 100.00          |
| direct_calls| 10     | 30.00 -> 114.29 (100 max) | 100.00 -> 100.00        |
| dynamic     | 1      | 0.00 -> 0.00            | 0.00 -> 0.00              |
| exceptions  | 3      | 0.00 -> 100.00          | 0.00 -> 100.00            |
| external    | 2      | 100.00 -> 100.00        | 100.00 -> 100.00          |
| functions   | 4      | 25.00 -> 100.00         | 100.00 -> 100.00          |
| generators  | 17     | 41.18 -> 60.87          | 100.00 -> 100.00          |
| imports     | 14     | 100.00 -> 100.00        | 100.00 -> 100.00          |
| kwargs      | 10     | 20.00 -> 123.08 (100 max) | 100.00 -> 100.00        |
| lambdas     | 14     | 35.71 -> 52.63          | 100.00 -> 100.00          |
| lists       | 13     | 38.46 -> 57.14          | 100.00 -> 100.00          |
| mro         | 14     | 71.43 -> 95.24          | 100.00 -> 100.00          |
| returns     | 12     | 66.67 -> 100.00         | 100.00 -> 100.00          |

(The per-category percentages can exceed 100 because the scorer's ours
rows dedupe on the 4-column join; every deduped ours row matches the
oracle — precision is exactly 100.)

### Written stops — the shapes this tier refuses, and why

These residual categories carry dynamic shapes that syntax alone cannot
carry honestly. Each is a stop, not a guess; taking them needs a
dataflow or checker tier.

- **Cross-function container/param flow** (`dicts/param`,
  `dicts/param_key`, `lists/param_index`): the container or key travels
  through a parameter binding across defs. Same-file byte-order bindings
  cannot see it.
- **Call-result containers** (`dicts/return`, `dicts/return_assign`,
  `lists/slice`): the container comes from a call result or a slice of
  another container; no binding row covers a non-literal rhs.
- **Computed keys** (`dicts/new_key_param`, `dicts/type_coercion` —
  ambiguous int/str key collision is dropped by rule,
  `dicts/ext_key`, `lists/ext_index`): non-literal keys.
- **Mutation** (`dicts/update`): a method call mutates the container; we
  kill the binding and stay silent.
- **Chained default/param fan-out** (`kwargs/chained_call` partial): the
  arg value reaching a callee's parameter through another call's arg
  needs two interprocedural hops (we carry one; `func2->func3` lands,
  the synthetic `func2 -> func2` identity edge does not).
- **Higher-order through class attributes** (`classes/assigned_call`,
  `classes/return_call*`): receiver attr dispatch, residual group 2.
- **`dynamic/eval`**: not syntax, by definition.

Receipt per stop: `python-oracle/MISSES.tsv` rows per case, plus the
per-case rows in SCORES.tsv.

### Original scoring — aggregate (before the module-caller fix, exact 4-column match)

- Oracle edges: 236 (after dedup; 20 external rows excluded from the
  denominator — `<builtin>`/builtin-type callees)
- Ours edges: 17 (deduped `resolved_edge` rows across the whole suite)
- Overlap: 14
- Recall: 5.93 %, precision: 82.35 % (exact). Fuzzy (Jaccard 0.8) does not
  move recall materially: the misses are missing edges, not misnamed ones.

### Rescore after the module-caller fix (fix-extract-python-module-resolve, 2026-08-31)

The dominant defect below is fixed: `PythonSource::resolve` no longer drops
a site with no covering def — `project_call` mints a nameless whole-file
module def (`python::MODULE_CALLER`, an Ext kind so the one-language kind
rail holds), and `caller_name` answers null for it. The null is the bench
join's empty src_name, so module rows join the oracle exactly. Two python
callee rulings landed with it (both in `PythonSource::call_name_match`):

- A constructor call `C()` resolves to `C.__init__` (same file, else a
  unique corpus blob; span containment over the class def). Resolving to
  the class TypeF def minted class-name rows the oracle never has — 37 of
  136 rows of false precision in the first post-fix run.
- The bare-name fallback is CALL-family only: a name that is only a class
  no longer resolves to the class def.
- A class with no explicit `__init__` resolves to nothing (the implicit
  `object.__init__` has no def site to point at; PyCG internalizes it).

One pinned contract flipped with a receipt: `corpus_8.py`'s `Widget()` now
names `__init__`, never `Widget` (tests/47_resolve_door_cli.rs).

Aggregate: oracle 236, ours 115, overlap 114 — recall 48.31 % (+42.38),
precision 99.13 % (+16.78). Before/after per category (exact; oracle edges
are the per-case sums, not case counts):

| category    | oracle | recall before -> after | precision before -> after |
|-------------|--------|------------------------|---------------------------|
| args        | 14     | 0.00 -> 42.86          | 0.00 -> 100.00            |
| assignments | 15     | 0.00 -> 0.00           | 0.00 -> 0.00              |
| builtins    | 4      | 25.00 -> 25.00         | 100.00 -> 100.00          |
| classes     | 48     | 12.50 -> 77.08         | 75.00 -> 100.00           |
| decorators  | 22     | 0.00 -> 36.36          | 0.00 -> 88.89             |
| dicts       | 19     | 0.00 -> 26.32          | 0.00 -> 100.00            |
| direct_calls| 10     | 0.00 -> 30.00          | 0.00 -> 100.00            |
| dynamic     | 1      | 0.00 -> 0.00           | 0.00 -> 0.00              |
| exceptions  | 3      | 0.00 -> 0.00           | 0.00 -> 0.00              |
| external    | 2      | 0.00 -> 100.00         | 0.00 -> 100.00            |
| functions   | 4      | 0.00 -> 25.00          | 0.00 -> 100.00            |
| generators  | 17     | 0.00 -> 41.18          | 0.00 -> 100.00            |
| imports     | 14     | 14.29 -> 100.00        | 100.00 -> 100.00          |
| kwargs      | 10     | 0.00 -> 20.00          | 0.00 -> 100.00            |
| lambdas     | 14     | 14.29 -> 35.71         | 100.00 -> 100.00          |
| lists       | 13     | 0.00 -> 38.46          | 0.00 -> 100.00            |
| mro         | 14     | 7.14 -> 71.43          | 100.00 -> 100.00          |
| returns     | 12     | 16.67 -> 66.67         | 100.00 -> 100.00          |

(Ours edges: 17 -> 115. The single remaining ours-only row is the
pre-existing `decorators/return_different_func` name-match false
positive.)

### Residual 122 misses, three shapes, all callee-side

1. **Callee-side value flow (79 rows)**: `b = func1; b()`,
   `func(func2, c=func4)` args/kwargs dispatch, decorator return values,
   `func()()` chains, tuple/starred unpacking, container dispatch. The
   caller side is fixed; the callee needs simple-assignment value
   propagation (a phase-1 alias fact + resolve join, its own arc).
2. **Receiver-type dispatch (17 rows)**: `a.func()` after `a = MyClass()`,
   MRO-inherited `__init__`/methods through base classes.
3. **Dunder iteration + dynamic (26 rows)**: `for i in x:` -> `__iter__`/
   `__next__` synthesis, generators' yield return types, `raise A`, eval.

Recall 48.31 sits under the lane's "well above 50 %" target: the 185
module-src rows the original report attributed to the caller gap include
these callee-side shapes, and caller attribution alone cannot recover
them. Next arc: the phase-1 alias table (residual group 1 alone lifts
recall past 54 %).

### (original run) The one dominant failure: module-level call sites

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

### (original run) 3 worst categories, 5 example misses each

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

### (original run) Shape vs PyCG's published 103/112

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
