# Lane `bench-extract-single-process` (opus): every "ours" number re-measured from ONE process per corpus

Read `plans/extract-bench-2026-08-29/COMMON.md`, `ORACLES.REPORT.md` section 13 finding 1, and
`docs/failure-modes.md` entry 96. `resolve_runs.py` groups files by
directory depth and splits on rc=124, so every `<lang>.parse.{call,type}.tsv`
and `<lang>.dietscip.*.tsv` was produced by many processes and loses every
cross-partition edge (go: 0 of ours cross a top-2 directory, oracle 1,908;
rust: 0 cross a crate). Every recall number for "ours" in ORACLES.REPORT.md
and TOOLS.REPORT.md is therefore a floor, not a measurement.

## First action
```
git merge --ff-only 0192e4d28f546a254eca76009f96e21e1eeafe61
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```

## Task
1. For each corpus run ONE process: `timeout 60 extract --resolve --family call,type,module <all files>`
   (module family name: check `extract --help`; go used `--deps` for
   file edges, see ORACLES.REPORT.md section 11). Run it in background with
   a log, record wall and peak RSS (`/usr/bin/time -l`). Argument list
   length: pass a file list via whatever the binary supports (`--help`),
   else xargs with `-s` large enough that ONE invocation carries every
   path; verify with `wc -l` on the run log that exactly one process ran.
2. Wall over 10 s is a defect under the 10-second law, and you still take
   the numbers: report the wall, then `cargo flamegraph` or `samply` (if
   installed) or `sample <pid> 5` for a 5 s profile, and list the top 5
   frames in the report. Do not fix src.
3. Regenerate `<lang>.parse.{call,type,module}.tsv` and
   `<lang>.dietscip.{call,type}.tsv` from the single-process output, using
   the same normalisation as before (`normalize.py`; keep the old files as
   `*.chunked.tsv` for the diff).
4. Recompute every table row that has an "ours" column against every
   oracle and tool tsv present: `<lang>.oracle.*.tsv`, `*.codeql2.*.tsv`,
   `*.joern2.*.tsv`, madge/depcruise. Append section 14 "single process"
   to ORACLES.REPORT.md: one table per language, rows = family x oracle,
   columns = chunked recall, single-process recall, precision, wall, RSS.
5. Rewrite `resolve_runs.py` so its default is one process per corpus
   and the split path is opt-in (`--chunk`), with the docstring saying why.

## Ownership
`plans/extract-bench-2026-08-29/*` except `TOOLS.REPORT.md`. No `src/`, no other `plans/`.
The 10-second law applies to each extract invocation you WAIT on: over 60 s
kill it, report, move on; never raise the timeout.

## Receipt
Push `bench/extract-single-process`, `gh pr create --base main`, hail
`boop beep --no-wait --as bench-extract-single-process sprefa-coordinator "single process: PR #N, go call x%->y%, ts call x%->y%, rust call x%->y%, walls go/ts/rust s"`.
Laws: no em dashes, no words provenance/substrate/load-bearing/regime, never
"ground truth" (say oracle), commit the tsvs, no `--no-verify`.
