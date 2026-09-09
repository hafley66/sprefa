# Lane `research/rust-codeql-gap` (glm53): census the rust checker-tier gap vs the CodeQL oracle

First action: `git merge --ff-only e89b986c79122cb7e826a022b01e2ecca9d9cc5a`. Failure = STOP, beep the coordinator.

## Goal, one sentence

Classify every CodeQL-oracle call row the checker-tier arm misses on the
rust-analyzer corpus into named syntactic/semantic classes with counts and 3
cited examples each, so the user can pick which class to fund; ZERO extractor
changes.

## Why

User word 2026-08-31: "we _want_ non syntax tier working at codeql level."
Current floor (RATCHET.tsv): rust call vs `rust.codeql.call.tsv` recall 73.37 /
precision 78.71 WITH the checker. The miss set is ~11.7k rows and nobody has
named its classes. Expected blocks: macro_rules expansion (OPEN-PROBLEMS row 11
pattern), dyn/trait dispatch, External-suppression misfires — but expect
surprises and COUNT them, never assume.

## Method (this is analysis, read-only on src/**)

1. Build once: `cd v6/sprefa-extract && nice -n 15 cargo build --release
   --features cli,rust-checker` (background, log the tail).
2. Emit ours: run the release binary with `--resolve --family call
   --rust-checker --project-root /Users/chrishafley/projects/rust-analyzer`
   over the corpus file set (the bench rule: every `.rs` under `crates/` whose
   path carries a `src` component; copy the enumeration from
   `tests/bench/mod.rs` `wants`). `nice -n 15`, background, 15-min cap.
3. Join against `plans/extract-bench-2026-08-29/rust.codeql.call.tsv` on the 4
   columns. Misses = oracle-only rows. Also keep ours-only rows for a
   contradicted/unjudged split (same (src_path, src_name), different dst =
   contradicted).
4. Classify misses by inspecting the source at the cited line. Class list is
   YOURS to discover; seed candidates: macro-minted callee, macro-site caller,
   trait-method via dyn, trait-method via generic bound, operator/Deref sugar,
   closure call, External-suppressed (we said External, CodeQL says corpus),
   oracle-convention (CodeQL naming we render differently). Every class gets:
   count, % of misses, 3 examples as `path:line` + the oracle row, and one
   sentence on what mechanism would answer it.
5. Commit the join/classify script as
   `plans/extract-bench-2026-08-29/rust.call_census.py` (same style as
   `rust.type_census.py`), so the census re-runs on any future binary.

## Files you own

- `plans/extract-bench-2026-08-29/rust.call_census.py` (new)
- `plans/extract-crawl-2026-08-29/rust.REPORT.md` — ONE new section, next free
  number (check the file head for the current max; two lanes have collided on
  section numbers before, renumber yours if taken)
- `plans/extract-bench-2026-08-29/OPEN-PROBLEMS.md` — update row 11 and the
  rust rows with the measured counts, same PR
- FORBIDDEN: v6/sprefa-extract/src/**, tests/**, RATCHET.tsv, the corpus
  (read-only, never git-touch ~/projects/rust-analyzer), root src/.

## Receipts (PR body)

1. The class table: name, count, % of the miss set, sums to 100%.
2. Contradicted vs unjudged counts for ours-only rows.
3. The exact binary sha (`git describe --always`) and corpus sha
   (`git -C ~/projects/rust-analyzer rev-parse HEAD`).
4. `git diff --stat origin/main...HEAD` shows only the three owned paths.

## Laws in force

- Banned words in prose AND identifiers: provenance, substrate, load-bearing,
  regime, ground truth (say oracle), honest, signal. No em dashes.
- Numbers carry units. Every claim cites path:line.
- Commit per logical step; push; `gh pr create`; then
  `boop beep --no-wait --as research-rust-codeql-gap sprefa-coordinator "<PR number, top 3 classes with counts>"`.
- 10-second law: nothing foreground over 10 s; extract runs and the build go
  background with caps. Do NOT run `just extract-ratchet` at all; this lane
  changes no code and needs no gate beyond the diff-stat check.
