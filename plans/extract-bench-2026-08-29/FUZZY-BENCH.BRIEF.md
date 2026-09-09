# Lane `fix-extract-bench-fuzzy` (glm53f): re-measure precision under name-tolerant matching

User decision 2026-08-31: run the fuzzy-mapping re-measure. Background:
`plans/extract-bench-2026-08-29/PRIOR-ART.md` sections 3 and 8 (read first).
Our bench joins ours-vs-oracle rows by EXACT 4-column string equality
(src_path, src_name, dst_path, dst_name). A row pointing at the right
function but spelling the name differently (`render` vs `Widget::render`)
scores as a miss. This lane measures how much of our precision/recall gap is
spelling penalty. REPORT-ONLY on the scoreboard: RATCHET.tsv floors do NOT
change; exact-match stays canonical.

## First action
```
git merge --ff-only <BASE_SHA from your spawn message>
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```

## Ownership (touch nothing else)
- NEW `plans/extract-bench-2026-08-29/fuzzy_bench.py`
- APPEND one section to `plans/extract-crawl-2026-08-29/rust.REPORT.md`
- NO changes under `src/**`, NO changes to RATCHET.tsv, NO changes to
  tests/** except none at all.

## Inputs
Oracle tsvs are committed under `plans/extract-bench-2026-08-29/`
(`rust.oracle.call.tsv`, `rust.codeql.call.tsv`, `rust.scip_override.call.tsv`,
`go.codeql2.call.tsv`, `go.oracle.call.vta.bare.tsv`, `ts.codeql2.call.tsv`,
`ts5.oracle.call.tsv`; ls the dir for exact names). The ours tsvs: read
`v6/sprefa-extract/tests/ratchet_recall.rs` and `tests/bench/mod.rs` to find
how the ratchet emits/derives ours rows and projections, and reproduce that
emission (go/ts build `--features cli`; rust build `--features cli,rust-checker`
and run the checker leg the way the ratchet does). Apply the SAME projections
the ratchet applies before comparing, or the numbers are not comparable.

## Method (three match modes, one script)
`fuzzy_bench.py <ours.tsv> <oracle.tsv> --mode exact|filepair|fuzzy`
1. exact: current 4-col equality. MUST reproduce the RATCHET.tsv row for each
   oracle to 0.01 pt; if it does not, STOP and report the delta, do not tune.
2. filepair: match on (src_path, dst_path) only, names ignored; count each
   ours row matched if the oracle has ANY row with that file pair (and
   vice-versa for recall). This is the name-blind upper bound.
3. fuzzy: within a (src_path, dst_path) group, similarity = Jaccard over
   identifier tokens (split names on ::, ., #, /, ->, and camelCase
   boundaries, lowercase). Greedy one-to-one assignment, highest similarity
   first; a pair counts as matched at similarity >= threshold. Report
   thresholds 0.8 and 0.5. One oracle row matches at most one ours row.

## Receipts (all in the rust.REPORT.md section, plus script output committed as
`plans/extract-bench-2026-08-29/fuzzy.RESULTS.tsv`)
- Table: lang x oracle x mode -> recall %, precision %. Rows: rust x3 oracles,
  go x2, ts5 x2. Exact column must equal RATCHET.tsv.
- Spelling-penalty column: (fuzzy@0.8 precision - exact precision) pt.
- 10 concrete example rows that miss at exact and match at fuzzy@0.8, with
  both spellings shown, for rust vs rust.oracle.call.tsv.
- One paragraph: is rust 55.98% precision mostly spelling penalty or mostly
  real disagreement? Answer with the numbers.

## Laws
- Wall/RSS: extraction runs background, nice -n 15, one at a time, 10 s cap
  per single operation (multi-fixture batteries exempt); machine must stay
  responsive.
- Python: plain argparse + stdlib, no deps. Descriptive variable names.
- Commit after each green step. Push branch, post PR to MAIN (never a stacked
  base). PR body carries the full table.
- Done or blocked: `boop beep --no-wait --as fix-extract-bench-fuzzy sprefa-coordinator "<one line>"`.
