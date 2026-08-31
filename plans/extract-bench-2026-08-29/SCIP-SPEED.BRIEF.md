# Lane `fix-extract-scip-speed` (glm53f): the scip-informed leg under the 10-second law, and go's scip normal form

Read `plans/extract-bench-2026-08-29/SCIP.REPORT.md` (PR #585). The
scip-informed resolve (`--resolve --scip-index <idx>`) reads ts call
coverage 88.64% (codeql 88.6%) and rust 87.95% (raw scip 93.2%), against
plain resolve 84.88% / 69.6%. It costs 113 s (ts), 163 s (rust), 436 s (go)
against 2.0 / 1.4 / 11.3 s plain. Hot frames: `scip::site_occurrence`
(1,820 + 1,759 of 3,940 samples on ts), `scip::definition_of` (484 of
3,524 on rust). Both are linear scans per site over the decoded index.

## First action
```
git merge --ff-only 128c72ad8bd1bc445c866cc64cb7b9f586e0b1bb
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```
Index paths are in SCIP.REPORT.md table 1; `out/scip_runs.sh` is the
receipt driver #585 wrote. Corpora and file lists: `out/*.files.txt`.

## Task A: index the index
In `src/scip*.rs`: decode once into (1) a per-document
`Vec<(start, end, symbol_id)>` sorted by start, so `site_occurrence` is a
binary search on (doc, offset); (2) a `HashMap<symbol_id, def_site>` so
`definition_of` is one lookup; symbols interned to u32 at decode. Keep the
public fn signatures. Receipt: three runs per corpus, the scip-informed wall
under 10 s for ts and rust (go: report the number; if over 10 s, profile and
name the next frame), and `sort a.jsonl | cmp - <(sort b.jsonl)` identical
before and after on every corpus.

## Task B: go scip rows read 9.1% against vta on every row of the table
Plain resolve reads 84.42% on `go.oracle.call.vta.bare.tsv` at #579, so the
9.1% in SCIP.REPORT.md is the normal form of that run (the earlier
`Type.Method` vs bare miss, ORACLES.REPORT.md section 12), not the
extractor. Fix the go rows in `out/scip_runs.sh` / the normalisation, rerun
the go table (plain / informed / raw scip), and correct SCIP.REPORT.md.

## Task C: make scip-informed the default when an index is fresh
`scip_freshness.rs` already knows whether an index matches the file set.
With Task A landed, `--resolve` uses the index when present and fresh, and
falls back to plain otherwise, with one `tracing::info` line saying which.
Fail-first test in `tests/scip_freshness.rs` or `tests/8_scip_families_cli.rs`.
Then `just extract-ratchet` (`tests/ratchet_recall.rs`) measures with the
index present: bump the ts and rust call rows with `RATCHET_BUMP=1` and
put the before/after in the PR body.

## Ownership
`v6/sprefa-extract/src/scip*.rs`, `src/bin/extract.rs` (flag plumbing only),
`tests/scip_freshness.rs`, `tests/8_scip_families_cli.rs`,
`tests/74_scip_relationship_family.rs`, `plans/extract-bench-2026-08-29/SCIP.REPORT.md`,
`out/scip_runs.sh`, `RATCHET.tsv`. NOT `src/lang/*` (a ts lane is live),
NOT `src/project.rs`, `src/types.rs`. No `cargo fmt` on files you do not
own. Every extract call under `timeout 60` for this lane (the pre-fix wall
is the defect being measured); a run over 60 s is killed and its partial
number reported. Gate in background with a log; wall-ratio flakes rerun 3x
isolated. No file over 1 MB. Budget 60 min; past it, post the PR with Task A.

Push `fix/extract-scip-speed`, `gh pr create --base main`, hail
`boop beep --no-wait --as fix-extract-scip-speed sprefa-coordinator "scip speed: PR #N, informed wall ts 113->x s rust 163->y s go 436->z s, go table fixed, gate a/b"`.
Laws: no em dashes anywhere, no eprintln (tracing only), descriptive names,
comments only for what code cannot show, no words
provenance/substrate/load-bearing/regime/refusal, never "ground truth".
