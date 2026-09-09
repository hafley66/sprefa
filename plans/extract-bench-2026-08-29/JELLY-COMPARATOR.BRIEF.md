# Lane `feat-extract-jelly-comparator` (glm53f): Jelly as a second ts/js call oracle

Prior-art sweep (plans/extract-bench-2026-08-29/PRIOR-ART.md section 9) found
Jelly (npm @cs-au-dk/jelly 0.13.x) ships static JS/TS call-graph emission and
a comparator. Job: produce a jelly oracle tsv for the ts5 corpus and score
our rows against it, alongside the existing tsc + codeql2 oracles.
REPORT-ONLY: no RATCHET.tsv change, no src/** change.

## Ownership (nothing else)
- NEW plans/extract-bench-2026-08-29/jelly.ORACLE.md (how it ran, versions,
  flags, caveats)
- NEW plans/extract-bench-2026-08-29/ts5.jelly.call.tsv (the oracle rows,
  4-col normal form, committed)
- NEW plans/extract-bench-2026-08-29/jelly_convert.py (jelly JSON -> 4-col
  tsv; argparse + stdlib only)
- APPEND one section to plans/extract-crawl-2026-08-29/ts.REPORT.md (create
  ts.REPORT.md if absent)

## Steps
1. `npm exec --yes @cs-au-dk/jelly -- --help` (or npx) to learn flags. Run
   jelly over the ts5 corpus (same corpus dir the ratchet ts5 leg uses; find
   it in tests/ratchet_recall.rs / tests/bench/mod.rs). Cap: background,
   nice -n 15, timeout 900 s, log to a file; if jelly exceeds 15 min or 4 GB
   RSS, kill it and report the numbers, do not wait it out.
2. Convert its call-graph JSON to 4-col tsv keyed like ours (src_path,
   src_name, dst_path, dst_name), paths relative to the corpus root. Apply
   the SAME projection rules the ratchet ts5 leg applies (read
   tests/bench/mod.rs) before comparing.
3. Score with plans/extract-bench-2026-08-29/fuzzy_bench.py --mode exact
   (already on main): ours vs jelly, and also jelly vs tsc-oracle and jelly
   vs codeql2 (oracle-vs-oracle agreement matters as context).
4. Receipts in the report section: table (pair, recall %, precision %, row
   counts), 10 example rows jelly has that both tsc and codeql2 lack (or the
   reverse), one paragraph: does jelly add discriminating signal beyond the
   two oracles we have, with numbers.

## Laws
- FORBIDDEN: v6/sprefa-extract/src/**, tests/**, RATCHET.tsv, justfile. A
  second live lane owns rust files; touch nothing outside your list.
- Numbers carry units. Commit after each green step. PR to MAIN.
- If jelly cannot handle the corpus (crashes, OOM, unsupported syntax),
  that IS a finding: report the exact error and the file count it managed,
  post the PR with what exists.
- Done/blocked: `boop beep --no-wait --as feat-extract-jelly-comparator sprefa-coordinator "<one line>"`.
