# sprefa-extract open problems (index, 2026-08-31)

Every open problem lives in a report section with numbers; this file is the
index. RULE for every future lane: closing or moving a problem updates this
file in the same PR; discovering one adds a row.

| # | problem | numbers | detail lives at | next action |
|---|---|---|---|---|
| 1 | rust type residual categories | micro-shape classes at 0% on the PyCG-style census: report sec 26 residual (post-#605 recall 61.70%) | rust.REPORT.md sec 26 | classify remaining 38.3% by shape, grind top classes |
| 2 | python dynamic shapes | aggregate recall 69.49% / precision 100.00% after the dynamic-shape tier (was 48.31/99.13); residuals: generators 60.87%, lambdas 52.63%, lists 57.14%, dicts 45.16% | py.REPORT.md (PyCG suite section) | written stops: cross-function container/param flow, call-result containers, computed keys, mutation, receiver dispatch — all need dataflow or checker tier |
| 3 | go agreed-missed residual | 351 rows: one-hop 155, multi-hop 139, alias 26, bare 12, qualified 11, promoted 8 | go.GAPS.md residual-six section | one-hop reassignment shape (x = y.M()) records no bind-plan row |
| 4 | ts madge module recall | 50.57% recall / 32.85% precision, never grinded | RATCHET.tsv ts5 module row | dependency-cruiser proposed as better oracle (PRIOR-ART.md sec 9) |
| 5 | rust checker RSS | 2.5 GB on ra corpus (foreign repos 330-934 MB) | rust.REPORT.md sec 24 | SHELVED by user 2026-08-31 ("whatevs for now") |
| 6 | corpus-scale name_resolve wrong-owner defect (go) | 13+58 wrong-owner bindings, corpus-scale dependent | GO-PARITY.REPORT.md sec 5a | folded into filed resolve defect; over 100-line bar |
| 7 | resolve is not a function of the input set | xargs chunking changed results (#600 found it; single-process forced) | rust.REPORT.md sec 27 header, PR #600 body | design-level: def index depends on file batch |
| 8 | bench protocol forks from prior-art study | 3-bucket precision (unjudged 0.4-11.1% miscounted), resolution-origin column | PRIOR-ART.md sec 3 | user decision pending |
| 9 | jelly oracle rejected | 35-37% agreement vs both established oracles | ts.REPORT.md jelly section, PR #603 | closed negative; keep tsv for reference |
| 10 | rust dyn-dispatch + fn-pointer kinds | rust-callgraph-benchmark: LLVM resolves dyn 0%, fn-ptr 0% (external finding) | PRIOR-ART.md sec 9 pass 1 | candidate micro-suite import like PyCG's |
