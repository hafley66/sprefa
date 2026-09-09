# Lane `fix-extract-python-dynamic` (glm53f): the dynamic-shape classes after the module fix

py.REPORT.md PyCG-suite section post-#610: recall 48.31%, precision 99.13%.
Residual classes by recall: assignments 0% (8 edges: tuple/starred/chained
unpacking of functions), exceptions 0% (3), dicts 26.32%, direct_calls 30%
(call of a call result), decorators 36.36%, lists 38.46%, args 42.86%
(function passed as param, then called), kwargs 20%, functions 25%. This
lane takes the shapes SYNTAX can honestly carry; the truly dynamic ones get
a written stop, never a guess. OPEN-PROBLEMS.md row 2 is yours; update it
in your PR.

## First action
```
git merge --ff-only <BASE_SHA from spawn message>
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```
Read: src/lang/python/ (post-#610 layout), tests/80_py_module_caller.rs
(the pattern to copy), plans/extract-bench-2026-08-29/python-oracle/
(scorer + MISSES.tsv), py.REPORT.md.

## Order of attack (one class per commit, fail-first each)
1. assignments (0%): `g = f` then `g()`; tuple unpack `a, b = f1, f2`. A
   same-file single-assignment alias is syntax-resolvable: track simple
   name-to-def aliases within one file, no flow analysis.
2. direct_calls (30%): `f()()` — the inner call resolves, the outer edge
   needs the return; only take the case where the called def's return is a
   same-file named function (PyCG's returns category shows the shape).
3. args (42.86%): parameter called inside the callee where EVERY call site
   in the corpus passes the same named function — the unique-candidate rule
   already used elsewhere; emit only on uniqueness.
4. decorators (36.36%): `@dec` where dec returns the wrapped function
   unchanged is already right; the miss is dec-defined-in-file returning a
   new def — take only the syntactic `def wrapper... return wrapper` shape.
5. dicts/lists: STOP unless a container is built once with literal function
   values and indexed with a literal key — else written stop in the report.
Each class: fixture in tests/fixtures/py_findings/<class>/, test
tests/80_py_<class>.rs with HEAD-failure receipt, rescore the suite, commit.

## Floors and laws
- precision >= 99.03% (99.13 - 0.10). A fix that guesses and drops precision
  gets reverted, not tuned.
- `just extract-ratchet` all rows hold (go/ts/rust untouched byte-identical).
- Ownership: src/lang/python/**, tests/80_py_*, tests/fixtures/py_findings/,
  plans/extract-bench-2026-08-29/python-oracle/ (rescore), py.REPORT.md,
  OPEN-PROBLEMS.md row 2. NOTHING else; another lane owns src/lang/rust*.
- nice -n 15; coordinate the final ratchet with the coordinator (another
  lane will also gate).
- PR to MAIN, before/after per-category table. Commit per green class.
Done/blocked: `boop beep --no-wait --as fix-extract-python-dynamic sprefa-coordinator "<one line>"`.
