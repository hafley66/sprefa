# Python trace oracle over the PyCG micro-suite (2026-09-03)

## TOC

1. What ran
2. Per-category: static recall vs recall-of-covered
3. Trace vs PyCG disagreements (all 16 rows)
4. Cases that did not run to completion (14 of 119)
5. Our misses against the trace (28 rows)
6. Receipts

## 1. What ran

| item | value |
|---|---|
| tracer | `run.py`, `sys.monitoring` (PEP 669) `PY_START` + `CALL`, tool id `PROFILER_ID`; `sys.setprofile` fallback below 3.12 (not exercised) |
| python | `Python 3.14.6` (`python3 --version`), `RUNS.tsv` column `python` |
| cases | 119 (`find suite -name callgraph.json`), each `main.py` in its own subprocess, cwd and `sys.path[0]` = case dir, stdin `/dev/null`, 10 s timeout |
| ran to completion | 105 of 119 (`grep -c '	ok	' RUNS.tsv`) |
| timeouts | 0 |
| trace edges | 220 (`TRACE.tsv` rows), across 105 ok + 9 partial cases |
| PyCG non-external edges | 236 |
| trace and PyCG | 220 (every trace row is a PyCG row) |
| trace-only rows | 0 |
| PyCG-only rows | 16 (sec 3) |
| wall | 5.5 s for all 119 (`time python3 run.py`) |
| scorer | `pycg_score.py --oracle trace` writes `SCORES.tsv`, `MISSES.tsv`; the default `--oracle pycg` path is byte-identical (`git status` clean on `oracle/`, `ours/`, `SCORES.tsv` after a rerun) |
| extract binary | `v6/sprefa-extract/target/release/extract` built at e08866c828 with `--features cli`; default-path TOTAL `oracle 236 external 20 ours 205 overlap 205 recall 86.86 precision 100.00` |

Row spelling: `path = <category>/<case>/<file>`, `name` = last qualname segment (`<locals>` and comprehension segments dropped), `""` for module-level code, `<lambdaN>` = 1-based pre-order index among the file's lambda code objects (matched by code-object equality against a fresh `compile()` of the file; `dont_inherit=True` is required or the caller's `__future__` flags poison the comparison).

## 2. Per-category: static recall vs recall-of-covered

`static recall` = `ours and PyCG / PyCG` from `../SCORES.tsv`. `recall-of-covered` = `ours and trace / trace` from `SCORES.tsv`. Buckets split `ours` on the caller key `(src_path, src_name)` as `tests/bench/mod.rs` `buckets` does.

| category | cases | ran ok | PyCG edges | trace edges | trace and PyCG | ours | ours and trace | static recall vs PyCG (pct) | recall-of-covered vs trace (pct) | matched | contradicted | unjudged |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| args | 6 | 6 | 14 | 14 | 14 | 11 | 11 | 78.57 | 78.57 | 11 | 0 | 0 |
| assignments | 4 | 4 | 15 | 15 | 15 | 15 | 15 | 100.00 | 100.00 | 15 | 0 | 0 |
| builtins | 3 | 2 | 4 | 0 | 0 | 4 | 0 | 100.00 | 0.00 | 0 | 0 | 4 |
| classes | 22 | 22 | 48 | 48 | 48 | 48 | 48 | 100.00 | 100.00 | 48 | 0 | 0 |
| decorators | 7 | 7 | 22 | 21 | 21 | 20 | 20 | 90.91 | 95.24 | 20 | 0 | 0 |
| dicts | 12 | 12 | 19 | 19 | 19 | 14 | 14 | 73.68 | 73.68 | 14 | 0 | 0 |
| direct_calls | 4 | 4 | 10 | 10 | 10 | 9 | 9 | 90.00 | 90.00 | 9 | 0 | 0 |
| dynamic | 1 | 1 | 1 | 1 | 1 | 0 | 0 | 0.00 | 0.00 | 0 | 0 | 0 |
| exceptions | 3 | 0 | 3 | 3 | 3 | 3 | 3 | 100.00 | 100.00 | 3 | 0 | 0 |
| external | 6 | 0 | 2 | 0 | 0 | 2 | 0 | 100.00 | 0.00 | 0 | 0 | 2 |
| functions | 4 | 4 | 4 | 4 | 4 | 4 | 4 | 100.00 | 100.00 | 4 | 0 | 0 |
| generators | 6 | 6 | 17 | 17 | 17 | 7 | 7 | 41.18 | 41.18 | 7 | 0 | 0 |
| imports | 14 | 14 | 14 | 14 | 14 | 14 | 14 | 100.00 | 100.00 | 14 | 0 | 0 |
| kwargs | 3 | 3 | 10 | 9 | 9 | 9 | 9 | 90.00 | 100.00 | 9 | 0 | 0 |
| lambdas | 5 | 2 | 14 | 10 | 10 | 12 | 8 | 85.71 | 80.00 | 8 | 1 | 3 |
| lists | 8 | 8 | 13 | 13 | 13 | 10 | 10 | 76.92 | 76.92 | 10 | 0 | 0 |
| mro | 7 | 7 | 14 | 14 | 14 | 14 | 14 | 100.00 | 100.00 | 14 | 0 | 0 |
| returns | 4 | 3 | 12 | 8 | 8 | 9 | 6 | 75.00 | 75.00 | 6 | 1 | 2 |
| TOTAL | 119 | 105 | 236 | 220 | 220 | 205 | 192 | 86.86 | 87.27 | 192 | 2 | 11 |

Reading the table:

- The two columns differ only where the run did not reach the PyCG edge: builtins (crash before any call), external (`ext` absent by design), lambdas (3 crashes), returns (1 crash), decorators and kwargs (2 PyCG over-approximations the run never takes, sec 3).
- On every category that ran clean and has no PyCG over-approximation, recall-of-covered equals static recall: the trace agrees with PyCG on all 220 executed edges, and our 28 misses against the trace are the same rows as our misses against PyCG.
- The 2 `contradicted` rows and 11 of the 13 non-matched `ours` rows sit in crashed cases (sec 4): the caller ran up to the crash, so the trace judged it, and the edge past the crash line never executed. Neither is a resolver error.
- `dynamic/eval`: the trace records `<module> -> func` through `eval("func()")` (the `<string>` frame is outside the case dir and is skipped as a caller), the same row PyCG carries; ours has no row.

## 3. Trace vs PyCG disagreements (all 16 rows)

Trace-only rows: 0. PyCG-only rows: 16.

| case | run status | side | src_name | dst_path | dst_name | why |
|---|---|---|---|---|---|---|
| builtins/map | error | PyCG only | `<module>` | main.py | `func` | suite defect: `map([1, 2, 3], func)` passes the list as the function, `TypeError` at line 4 before any in-case call |
| builtins/map | error | PyCG only | `<module>` | main.py | `func2` | same crash |
| builtins/map | error | PyCG only | `<module>` | main.py | `func3` | same crash |
| builtins/map | error | PyCG only | `func2` | main.py | `func` | same crash |
| decorators/nested_decorators | ok | PyCG only | `<module>` | main.py | `func` | PyCG over-approximation: line 17 `func()` calls `dec1`'s `inner`; the raw `func` is only reached through `inner -> func`, which both trace and PyCG carry |
| external/attribute_assigned | error | PyCG only | `<module>` | main.py | `fn` | `ext` module absent by design (`ModuleNotFoundError` at line 1); `fn` never defined |
| external/cls_parent | error | PyCG only | `<module>` | main.py | `fn` | same |
| kwargs/chained_call | ok | PyCG only | `func2` | main.py | `func2` | PyCG over-approximation: it unions the default `a=func3` with the passed `a=func2`; the run binds `a=func3` inside `func2`, so `func2 -> func3` is the only executed callee |
| lambdas/chained_calls | error | PyCG only | `func1` | main.py | `func2` | suite defect: `a()` calls `lambda x: x + 1` without `x`, `TypeError` at line 9 |
| lambdas/chained_calls | error | PyCG only | `func2` | main.py | `<lambda2>` | same crash |
| lambdas/chained_calls | error | PyCG only | `func2` | main.py | `func3` | same crash |
| lambdas/chained_calls | error | PyCG only | `func3` | main.py | `<lambda3>` | same crash |
| returns/return_complex | error | PyCG only | `<module>` | main.py | `func2` | suite defect: `func4()()` omits `func4`'s parameter, `TypeError` at line 16 |
| returns/return_complex | error | PyCG only | `<module>` | main.py | `func5` | same crash, line 21 never reached |
| returns/return_complex | error | PyCG only | `func4` | main.py | `func3` | same crash |
| returns/return_complex | error | PyCG only | `func5` | main.py | `func2` | same crash |

| bucket | rows |
|---|---|
| unreached because the case crashed on a suite defect | 12 |
| unreached because `ext` is absent by design | 2 |
| PyCG over-approximation (edge never executes) | 2 |

## 4. Cases that did not run to completion (14 of 119)

`RUNS.tsv` rows with `status != ok`. Edges recorded up to the failure line are kept in `TRACE.tsv`.

| case | status | detail | edges kept | class |
|---|---|---|---|---|
| builtins/map | error | `TypeError: 'function' object is not iterable` at main.py:4 | 0 | suite defect (map arguments reversed) |
| exceptions/raise | error | `A` raised at main.py:5 | 1 | by design: the case raises at module level; its one edge `<module> -> __init__` is complete |
| exceptions/raise_assigned | error | `A` raised at main.py:6 | 1 | by design, edge complete |
| exceptions/raise_attr | error | `B` raised at main.py:6 | 1 | by design, edge complete |
| external/attribute | error | `ModuleNotFoundError: ext` at main.py:1 | 0 | by design: PyCG models `ext` as an external module that does not exist on disk |
| external/attribute_assigned | error | same | 0 | by design |
| external/cls_parent | error | same | 0 | by design |
| external/function | error | same | 0 | by design |
| external/function_asname | error | same | 0 | by design |
| external/function_assigned | error | same | 0 | by design |
| lambdas/chained_calls | error | `<lambda>() missing 1 required positional argument: 'x'` at main.py:9 | 2 | suite defect |
| lambdas/parameter_call | error | same shape, main.py:2 | 2 | suite defect; both PyCG edges executed before the crash |
| lambdas/return_call | error | same shape, main.py:5 | 2 | suite defect; both PyCG edges executed before the crash |
| returns/return_complex | error | `func4() missing 1 required positional argument: 'a'` at main.py:16 | 2 | suite defect |

| class | cases | PyCG edges unreached |
|---|---|---|
| suite defect (the program crashes on its own code) | 5 | 12 |
| `ext` absent by design | 6 | 2 |
| raises by design, edges complete | 3 | 0 |
| `input()` / sleep / infinite loop timeouts | 0 | 0 |

## 5. Our misses against the trace (28 rows)

`MISSES.tsv` rows without `OURS_ONLY`. Every one is also a miss against PyCG (`../MISSES.tsv`); the stop names are OPEN-PROBLEMS row 2's.

| case | src_name | dst_name | stop (OPEN-PROBLEMS row 2) |
|---|---|---|---|
| args/imported_assigned_call | `func` (to_import.py) | `param_func` | cross-file param flow |
| args/imported_call | `func` (to_import.py) | `param_func` | cross-file param flow |
| args/param_call | `func` | `func3` | call-valued args |
| decorators/nested_decorators | `inner` | `inner` | decorator closures |
| dicts/ext_key | `<module>` | `func` | computed or external keys |
| dicts/new_key_param | `<module>` | `func2` | computed or external keys |
| dicts/param_key | `func1` | `func2` | multi-target union |
| dicts/param_key | `func1` | `func3` | multi-target union |
| dicts/return_assign | `<module>` | `func2` | call-valued slots |
| direct_calls/imported_return_call | `<module>` | `return_func` (to_import.py) | cross-file return flow |
| dynamic/eval | `<module>` | `func` | eval |
| generators/iter_param | `func` | `__iter__` | receiver dispatch to `__iter__`/`__next__` |
| generators/iter_param | `func` | `__next__` | same |
| generators/iter_return | `<module>` | `__iter__` | same |
| generators/iter_return | `<module>` | `__next__` | same |
| generators/iter_return | `<module>` | `func` | call-valued for target |
| generators/iterable | `<module>` | `__iter__` | receiver dispatch |
| generators/iterable | `<module>` | `__next__` | receiver dispatch |
| generators/iterable_assigned | `<module>` | `__iter__` | receiver dispatch |
| generators/iterable_assigned | `<module>` | `__next__` | receiver dispatch |
| generators/yield | `<module>` | `func2` | yield-to-loop-target |
| lambdas/calls_parameter | `<lambda1>` | `func1` | multi-target union |
| lambdas/calls_parameter | `<lambda1>` | `func2` | multi-target union |
| lists/ext_index | `<module>` | `func2` | computed or external keys |
| lists/param_index | `func1` | `func2` | computed or external keys |
| lists/slice | `<module>` | `func2` | slices |
| returns/imported_call | `<module>` | `return_func` (to_import.py) | cross-file return flow |
| returns/nested_import_call | `<module>` | `return_func` (to_import2.py) | cross-file return flow |

## 6. Receipts

| command | prints |
|---|---|
| `python3 plans/extract-bench-2026-08-29/python-oracle/trace/run.py` | 119 case lines then `TOTAL cases 119 ok 105 edges 220 python Python 3.14.6`; rewrites `TRACE.tsv`, `RUNS.tsv` |
| `python3 plans/extract-bench-2026-08-29/pycg_score.py --oracle trace --extract <bin>` | the sec 2 table (without the static column) on stdout; rewrites `trace/SCORES.tsv`, `trace/MISSES.tsv`, `ours/*.call.tsv` |
| `python3 plans/extract-bench-2026-08-29/pycg_score.py --extract <bin>` | `TOTAL oracle 236 external 20 ours 205 overlap 205 recall 86.86 precision 100.00`; `git status` shows no change under `python-oracle/{oracle,ours,SCORES.tsv,MISSES.tsv}` |
| `wc -l plans/extract-bench-2026-08-29/python-oracle/trace/TRACE.tsv` | 221 (header + 220) |
| `grep -c '	ok	' plans/extract-bench-2026-08-29/python-oracle/trace/RUNS.tsv` | 105 |
