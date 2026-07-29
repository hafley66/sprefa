# crawl bench brief (codex luna): the grafana repo/rev crawl yardstick, both engines

The v5 yardstick exists only as memory-doc numbers: org-fan scan = 42,739
files / 389 repos / 5.9s cold (~7,244 files/s) over ~/orgs/grafana
(plans/2026-07-27-v5-repo-rev-receipts.md section: "No in-tree grafana crawl
benchmark file exists today" — true for BOTH engines). This lane makes the
crawl bench a repeatable in-tree script with a shared results table, v5 leg
and v6 leg.

## Deliverables
1. `v6/tsv2/scripts/crawl-bench.sh` + results doc `v6/tsv2/CRAWL-BENCH.md`.
2. **v5 leg**: hermetic org-fan crawl over ~/orgs/grafana — write a scratch
   config TOML ([[org]] dir = ~/orgs/grafana, read the config shape from
   src/config.rs docs) into the bench workdir, then
   `SPREFA_CONFIG=<scratch>.toml DL_NO_DAEMON=1 DL_NO_FETCH=1
   DL_STATE_DIR=<scratch> target/release/dl <bench program> --db <scratch>`
   where the program is the receipts doc's org-fan shape:
   `src(p, rev) <- scan(r, "HEAD", "**/*.{go,ts,tsx}", p, rev), repo(r, _, _).`
   (adapt arity to the real syntax; docs/reference/syntax.md is the
   authority). Measure: cold wall time, file-row count, files/s, RSS peak
   (/usr/bin/time -l), db size.
3. **v6 leg**: the served tsv2 engine + extraction plane over the SAME
   corpus. v6 has NO org fan-out spelling (a known expressiveness gap —
   STATE it in the doc, do not fake it in-language): the script loops repos
   at the shell level and drives the enumerate/extraction hosts per repo
   (v6/tsv2/scripts/extraction-live.sh and enumerate.sh show the working
   host + arrival shapes). Measure the same columns, plus stmts/tick from
   DL_PERF_LOG if cheaply available.
4. **The parity table**: one table, columns engine / files / repos / wall /
   files-per-s / RSS / db size, with the memory-doc v5 numbers quoted as a
   third historical row. Plus a gaps section: every place the two legs are
   NOT measuring the same thing (v5 scan facts vs v6 extraction families;
   fan-out in-language vs shell loop; digest models), stated plainly.
5. justfile recipe `crawl-bench` — NOT in green-all (it is a bench, minutes
   long, corpus outside the repo).

## Laws
- ~/orgs/grafana is READ-ONLY: never write there, never fetch/clone
  (DL_NO_FETCH=1 everywhere; if a repo is missing/shallow, skip and count
  skips). Never touch ~/.local/state/sprefa or any daemon.
- The whole bench runs under `nice -n 19` (machine-seize law; the user was
  bitten today). Both legs.
- SLOT-CORPUS-SCOPE (yours to fill, state the fill): if the full 389-repo
  crawl makes the v6 leg unreasonably long (>15 min), the script takes a
  repo-count cap flag, the default run uses the cap, and the doc's table
  says which scope each row measured. The v5 leg is cheap — always full.
- Files: NEW crawl-bench.sh + CRAWL-BENCH.md + one justfile recipe line.
  Nothing else. Other lanes own the compiler and the flow rig.
- No new deps.

## Validation
- Script runs end to end twice (cold-ish repeatability; second run's numbers
  in the doc too). Exit 0. Shellcheck-clean-ish (bash -n at minimum).

## Final summary shape
Base sha; the exact v5 program text used; the parity table verbatim; the
gaps section verbatim; skip counts; wall time of your own two runs.
