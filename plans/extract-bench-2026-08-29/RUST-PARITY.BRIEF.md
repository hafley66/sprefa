# Lane `bench-extract-rust-parity` (glm53f): two rust call oracles on one projection, then the excess set

Goal (user, 2026-08-30): our rust call rows scored against TWO oracles with
one consistent projection. codeql's rust extractor panics on this corpus
(TOOLS.REPORT.md), so the pair is ra_ap_ide call hierarchy
(`rust.oracle.call.tsv`, 27,003 rows, built by `ra_ide_probe/`) and the
rust-analyzer scip index (`rust.scip_override.call.tsv`, raw scip; also
SCIP.REPORT.md "raw scip 93.2%"). RATCHET.tsv reads rust call 67.56 / 33.68
vs ra_ap_ide. Known scope facts to verify first:

1. ~8,650 of 11,470 `ambiguous` sites target std or deps
   (`rust.REPORT.md` section 16); no corpus row can be correct for them.
2. ra_ap_ide's call hierarchy is per-crate; cross-crate edges may be
   missing on the oracle side (STUDY.human.unga.md section 4).
3. Oracle rows whose dst_path is outside `crates/*/src/**` are outside the
   corpus and cannot be hit (session note 2026-08-30).

## First action
```
git merge --ff-only <BASE_SHA>
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```
Corpus `/Users/chrishafley/projects/rust-analyzer` (read-only), files: every
`.rs` under `crates/` with a `src` path component (873). ONE process,
`--resolve --project-root`, `timeout 30`, background with a log. Read
`COMMON.md`, `normalize.py`, `bench.py`, `tests/bench/mod.rs` (`normal_form`
line 170), `tests/ratchet_recall.rs`, `rust.REPORT.md` sections 16-19.

## Task 1: the projection
`plans/extract-bench-2026-08-29/rust.project.py` (stdlib python only):
- `--scope corpus`: drop oracle rows whose dst_path is outside the corpus
  file list; drop ours rows whose src_path is not in the oracle's src_path set.
- `--closure enclosing`: `closure@<n>` src_name rows dropped when the
  mirrored enclosing-fn row exists.
- `--generic`: strip turbofish/generic suffixes from names on all sides
  if either oracle carries them (check with `grep -c '<' <tsv>` first).
Rerun the numbers for BOTH oracles, ours projected, and write
`plans/extract-bench-2026-08-29/RUST-PARITY.REPORT.md`:

| oracle | projection | recall | precision | ours rows | oracle rows | overlap |

Compute recall = overlap/oracle, precision = overlap/ours yourself
(`bench.py` labels are swapped, ORACLES.REPORT.md:583). Also the row
`ra_ap_ide vs scip` (oracle vs oracle): how much the two oracles agree
bounds what "parity" can mean.

## Task 2: the ratchet measures the same thing
Port the projection into `tests/bench/mod.rs` behind a `RustProjection`
struct; add a second rust call row to RATCHET.tsv for
`rust.scip_override.call.tsv`; `tests/ratchet_recall.rs` applies the
projection to both rust call rows. Fail-first unit test on a 6-row hand set.
`RATCHET_FORCE=1 just extract-ratchet` for rust rows ONLY (ts5 and go rows
byte-identical). Before -> after in the PR body.

## Task 3: the excess, classified
After projection, `ours − ra_ap_ide` and `ours − scip`, 300 each (seed 7):
wrong target (same name, other def), trait fan-out beyond the oracle's one
pick, macro-expanded site, generated file, method value not call, other.
Table with count, two file:line each, the `rust*.rs` fn that emits it. Fix
the top class fail-first if under 100 lines (`tests/7N_rust_<name>.rs`,
fixture under `tests/fixtures/rust_findings/`); otherwise write
`plans/extract-crawl-2026-08-29/rust-excess-1.FIX.BRIEF.md`.

## Ownership
`plans/extract-bench-2026-08-29/rust.project.py`, `RUST-PARITY.REPORT.md`,
`RATCHET.tsv` rust rows only, `tests/bench/mod.rs` (rust parts; a go lane may
own `GoProjection` in the same file: append, never reorder, and rebase on
origin/main before posting), `tests/ratchet_recall.rs` rust fns,
`tests/7N_rust_*.rs`, `src/lang/rust*.rs`, `plans/extract-crawl-2026-08-29/rust*`.
NOT `src/lang/ts*`, `go.rs`, `go_modules.rs`, `scip*`, `src/types.rs`,
`src/project.rs`. No `cargo fmt` on files you do not own. Rust wall under
10 s (3-run median). Gate in background with a log; wall-ratio flakes rerun
3x isolated. No file over 1 MB. Budget 90 min; post the PR with Tasks 1-2 at
90 min.

Push `bench/extract-rust-parity`, `gh pr create --base main`, hail
`boop beep --no-wait --as bench-extract-rust-parity sprefa-coordinator "rust parity: PR #N, vs ra_ap_ide r/p a/b -> c/d, vs scip c/d, oracle agreement e, top excess <name> n, gate x/y"`.
Laws: no em dashes anywhere, no eprintln (tracing only), descriptive names,
comments only for what code cannot show, no words
provenance/substrate/load-bearing/regime/refusal, never "ground truth".
