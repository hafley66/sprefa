# PyCG micro-benchmark suite as our first python call-graph oracle

- Upstream: github.com/vitsalis/PyCG, Apache-2.0, ARCHIVED (frozen).
- Pinned sha: `8d5dc40837803beef1d8d379fbf2cdad6cd94641` (shallow clone at
  ~/corpora/PyCG, 2026-08-31).
- Suite copy: `python-oracle/suite/` = upstream `micro-benchmark/snippets/`
  verbatim: 119 case dirs in 18 categories, each `main.py` (+ optional
  helper files) plus `callgraph.json` with expected edges. License kept at
  `python-oracle/PyCG.LICENSE` (upstream ships it as `LICENCE`).
- Scoring scripts: `../pycg_convert.py` (oracle conversion), `../pycg_score.py`
  (ours + per-case scoring). Output: `python-oracle/SCORES.tsv`,
  `python-oracle/MISSES.tsv`, `python-oracle/oracle/*.call.tsv`,
  `python-oracle/ours/*.call.tsv`.

## Oracle tsv mapping rules (pycg_convert.py)

PyCG qualnames are `<module>.<rest...>` with a dotted in-module qualname
(e.g. `main.MyClass.func`, `nested.to_import.func`). Conversion to our
4-column edge rows (src_path, src_name, dst_path, dst_name):

1. Module resolution. Per case dir, index every `.py` file as its dotted
   path without `.py` (`a/b.py` -> `a.b`) and every package dir
   (`x/__init__.py`) additionally as the dir's dotted path (`x`). The
   LONGEST dotted prefix of the qualname that hits the map is the module;
   it resolves to a FILE. The remainder is the in-file qualname.
2. Name = last segment of the in-file qualname. PyCG keeps the class path
   (`main.MyClass.func1`); our resolver reports bare callee names
   (`func1`), so the class prefix is dropped on both sides. The fuzzy
   columns in SCORES.tsv carry the residual class ambiguity.
3. Pure module nodes (src `main`, dst `nested.mod`) map to name = ""
   (module-level code); our arm reports a null caller_name there.
4. `<lambdaN>` segments are real lambda qualnames inside a file; they stay
   internal with the segment kept verbatim.
5. A first segment starting with `<` other than `<lambdaN>`
   (`<builtin>`, `<**PyStr**>`, `<**PyDict**>`) has no file in the suite:
   dst_path = `<external>`, dst_name = full original qualname. These rows
   are EXCLUDED from recall denominators. Across the suite: 20 such
   oracle edge slots collapse to 20 deduped external rows (builtins 9:
   len/map/eval/range/super/StopIteration/function; builtin-type methods
   3: join/split/items; dynamically-typed module attrs 8 counted as
   internal where the module file exists, e.g. `ext.*` cases without a
   local ext.py -> external). The exact per-case counts are in
   SCORES.tsv's `external_edges` column (20 total).
6. Rows are deduplicated; paths are relative to the suite root, prefixed
   `category/case/`, so they line up 1:1 with the paths handed to
   `extract --resolve` (run from the suite root).

## Run shape

- Ours: `v6/sprefa-extract/target/release/extract --resolve <all case .py
  files>` (worktree-local extractor; the ~/.cargo/bin one predates the
  python resolve arm). 10 s cap per case; every case finished in well
  under 1 s. `resolved_edge` rows become (caller_path, caller_name or "",
  callee_path or "", callee_name or ""), deduped.
- Zero-edge oracle cases (12 of 119) are counted with recall 0/0 -> 0.00
  and never skipped: builtins/functions, builtins/types,
  external/attribute, external/function, external/function_asname,
  external/function_assigned, imports/import_as, imports/parent_import,
  imports/relative_import, imports/simple_import,
  imports/submodule_import, imports/submodule_import_as. Their callgraph
  key sets are pure module nodes; the module-import plane itself is
  scored by edges where a call crosses into an imported file.
