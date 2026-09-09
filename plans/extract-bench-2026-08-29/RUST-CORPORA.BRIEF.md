# Lane `bench-extract-rust-corpora` (glm53f): ratios across MULTIPLE real rust repos

User 2026-08-31: "pull multiple repos of rust worth down, that is kinda the
point, see what kind of ratios we can hit." Today every rust number comes
from ONE corpus (rust-analyzer's own crates/*/src, which is also the oracle's
home turf). This lane measures generalization: same arms, 5 more repos.

## Ownership (nothing else; a live lane owns src/lang/rust*)
- NEW plans/extract-bench-2026-08-29/rust-corpora/ (per-repo oracle tsvs,
  RESULTS.tsv, CORPORA.md with pinned shas)
- APPEND one section to plans/extract-crawl-2026-08-29/rust.REPORT.md
- FORBIDDEN: v6/sprefa-extract/src/**, tests/**, RATCHET.tsv, justfile.
REPORT-ONLY. If you find a defect, file it in the report, do not fix it.

## Corpora (clone shallow, pin the sha you got, record it)
Into ~/corpora/ (create; NOT inside the repo): ripgrep, tokio, serde, clap,
alacritty. Skip any repo whose checker-tier run exceeds the caps below;
record the numbers and move on. File set per repo: src/** + crates/*/src/**
.rs files, list the count.

## Method (mirror the ratchet rust leg; read tests/ratchet_recall.rs and
tests/bench/mod.rs and rust.REPORT.md section 24 for the exact recipe)
1. Oracle per repo: the same emitter that produced rust.oracle.call.tsv
   (ra_ap_ide based; find it via rust.REPORT.md; if it is a bin in the repo,
   build it once). Same projection rules as the ratchet.
2. Ours per repo, TWO arms: diet (default features) and checker
   (--features cli,rust-checker, --rust-checker with project_root; NOTE the
   scip-informed trap in report section 24.7 — verify the run stayed diet by
   the record-kind census).
3. Score with fuzzy_bench.py --mode exact. Also record wall s and peak RSS
   MB per run (/usr/bin/time -l; field 1 is REAL).

## Caps (laptop is in use; guard is live)
- Everything nice -n 15, background, ONE run at a time, never concurrent
  with another lane's build. 15 min / 4 GB per single run; over = kill,
  record, continue.

## Receipts (RESULTS.tsv + report section)
Table: repo x arm -> files #, ours rows #, oracle rows #, recall %,
precision %, wall s, RSS MB. Plus: min/median/max recall across repos per
arm, and 5 example misses from the WORST repo classified by shape. One
paragraph: does the 93.68% recall generalize or was it home-turf?

## Laws
Commit after each repo. PR to MAIN. Numbers carry units. Done/blocked:
`boop beep --no-wait --as bench-extract-rust-corpora sprefa-coordinator "<one line>"`.
