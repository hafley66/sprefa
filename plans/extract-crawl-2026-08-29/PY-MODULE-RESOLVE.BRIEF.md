# Lane `fix-extract-python-module-resolve` (glm53f): module-level callers emit zero resolved edges

BLOCKED-BEHIND: chore/extract-cleanup-lang (owns src/lang/**). Spawn only
after it merges; base on the post-merge main sha.

py.REPORT.md (PyCG suite section): 185 of 225 missed oracle rows have an
empty src_name — a call at module top level. `--family call` FINDS the
sites; `--resolve` emits no resolved_edge for a module-level caller.
Recall on the suite is 5.93% and this one defect owns most of it.

## Job
1. Fail-first: fixture under tests/fixtures/py_findings/module_caller/
   (main.py calling a local def + an imported def at top level), test
   `tests/80_py_module_caller.rs` asserting both edges, HEAD-failure
   receipt in the header.
2. Trace `--resolve` for python: find where the enclosing-caller lookup
   drops a site with no enclosing function (src/lang/python.rs; the
   caller attribution used by resolved_edge). Give module-level sites the
   module as caller: src_name = the file's module identity, matching what
   the 4-col bench join expects (empty src_name is what the oracle uses —
   decide WITH the bench in mind and document the choice in the test).
3. Rerun the PyCG suite scorer (plans/extract-bench-2026-08-29/
   python-oracle/, fuzzy_bench.py exact): update SCORES.tsv + the
   py.REPORT.md aggregate. Target: recall well above 50%; precision must
   not drop below 82.35% - 0.10 pt.
4. Ratchet: `just extract-ratchet` all rows hold (go/ts/rust untouched).

## Ownership
src/lang/python.rs (+ mod wiring if forced), tests/80_py_*, fixtures/
py_findings/, plans/extract-bench-2026-08-29/python-oracle/ (rescored),
py.REPORT.md. Nothing else.

## Laws
nice -n 15; suite green (flakes 3x isolated); PR to MAIN with
before/after per-category table. Done/blocked:
`boop beep --no-wait --as fix-extract-python-module-resolve sprefa-coordinator "<one line>"`.
