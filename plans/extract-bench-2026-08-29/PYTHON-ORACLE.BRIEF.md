# Lane `feat-extract-python-oracle` (glm53f): PyCG micro-suite as our first python oracle

Prior-art sweep (PRIOR-ART.md sections 8-9): we have NO python oracle. The
PyCG micro-benchmark (github.com/vitsalis/PyCG, Apache-2.0, ARCHIVED = frozen,
`micro-benchmark/snippets/`: 119 case dirs in 18 categories, each a main.py +
callgraph.json expected edges) is the field's only per-edge per-feature python
oracle. Import it, run our python arm over it, score per category. REPORT-ONLY.

## Ownership (a live lane owns src/lang/** — touch NOTHING there)
- NEW plans/extract-bench-2026-08-29/python-oracle/ (suite copy or convert
  output, SCORES.tsv, PYCG-SUITE.md with provenance sha + license note)
- NEW plans/extract-bench-2026-08-29/pycg_convert.py (callgraph.json ->
  4-col tsv; PyCG uses dotted qualnames — map to (file, name) per the
  suite layout; document the mapping rules in PYCG-SUITE.md)
- APPEND a section to plans/extract-crawl-2026-08-29/py.REPORT.md (create if
  absent)
- FORBIDDEN: src/**, tests/**, RATCHET.tsv, justfile.

## Steps
1. Shallow-clone PyCG to ~/corpora/PyCG, pin sha. Copy micro-benchmark/
   snippets into plans/.../python-oracle/suite/ (Apache-2.0, keep LICENSE).
2. Convert every callgraph.json to 4-col tsv. Rules doc: how a dotted
   qualname (pkg.mod.Class.method) becomes (path, name); builtins and
   stdlib callees map to dst_path=<external> and are EXCLUDED from recall
   denominators (state the count).
3. Run our python arm per case (`extract --resolve` over the case dir;
   read tests/ for the python invocation shape; nice -n 15, ONE case at a
   time is fine, they are tiny; 10 s cap each).
4. Score exact-mode with fuzzy_bench.py per case; aggregate per category.

## Receipts
SCORES.tsv: category, cases #, oracle edges #, ours edges #, recall %,
precision %. Report section: the table, the 3 worst categories with 5
example misses each, and one paragraph comparing our per-category shape to
PyCG's own published 103/112 sound (they fail on which categories, we fail
on which). Zero-edge cases counted, never skipped silently.

## Laws
Commit after steps 2 and 4. PR to MAIN. Numbers carry units.
Done/blocked: `boop beep --no-wait --as feat-extract-python-oracle sprefa-coordinator "<one line>"`.
