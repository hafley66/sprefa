# Grafana crawl bench

Run date: 2026-07-29

The script is [scripts/crawl-bench.sh](scripts/crawl-bench.sh). It uses a
hermetic scratch directory for both engines. The corpus at
`~/orgs/grafana` was read-only throughout. No fetch or clone was enabled.

## Scope

The corpus contained 389 Git checkouts. All 389 had a resolvable `HEAD` and a
successful `git ls-files` call. 139 had no tracked `go`, `ts`, or `tsx` files;
they were excluded from the v6 selected-repo list. The remaining 250 were
usable for the v6 leg.

The default `SLOT-CORPUS-SCOPE` fill is: v5 uses the full 389-repo org; v6
uses the first 8 usable repos in sorted path order. The v6 cap is exposed as
`--max-repos N`; `--max-repos 0` selects all usable repos. The v6 leg measured
779 files across 8 repos. At 40.68 files/s, a linear projection of the full
42,739-file v6 leg is 1,050.61s, which exceeds the 15-minute threshold. The
default cap keeps the repeatable run below that threshold.

The script and the justfile recipe invoke `nice -n 19`. The managed host
printed `nice: cannot set niceness: Operation not permitted` and continued the
child process; no higher-priority override was used.

## Exact v5 program

```dl
rel src(path: file, rev: text).
src(p, rev) <- scan(r, "HEAD", "**/*.{go,ts,tsx}", p, rev), repo(r, _, _).
```

The scratch config written for the v5 leg has this shape, with the absolute
corpus path substituted at runtime:

```toml
[[org]]
dir = "/Users/chrishafley/orgs/grafana"
```

The invocation sets `SPREFA_CONFIG`, `DL_NO_DAEMON=1`, `DL_NO_FETCH=1`, and a
scratch `DL_STATE_DIR`, then runs `target/release/dl <program> --db <scratch>`.

## v6 leg

The v6 program is generated in the scratch directory. It declares the existing
files host shape from `v6/dl/fixtures/files-hosts.dl6` and an extraction
host with this command shape:

```text
extract(path) -> sprefa-extract --family cst,type,call,df <repo>/<path>
```

The shell script loops over the selected repos. For each repo it starts one
served tsv2 process, sends three `want` arrivals for `**/*.go`, `**/*.ts`, and
`**/*.tsx`, waits for the `file` and `extracted` relations to settle, reads the
per-process `DL_PERF_LOG`, and closes the process. The extraction host emits
one success row per extracted file. The v6 program has no org fan-out spelling;
the shell loop supplies that missing operation at the orchestration boundary.

## Parity table

Run 1 of the final script version. RSS and database sizes are bytes. The
historical row is quoted from the v5 memory document.

| engine | files | repos | wall | files-per-s | RSS | db size |
|---|---:|---:|---:|---:|---:|---:|
| v5 org-fan, full 389-repo scope | 42,739 | 389 | 12.07s | 3,540.93 | 367,230,976 B | 52,371,456 B |
| v6 served + extraction, first 8 usable repos | 779 | 8 | 19.15s | 40.68 | 174,014,464 B | 1,069,056 B |
| v5 memory doc, historical full scope | 42,739 | 389 | 5.9s | 7,244 | flat | not recorded |

Repeat run 2 of the final script version:

```text
engine  files  repos  wall    files-per-s  RSS          db size
v5      42739  389    11.93s  3582.48      344391680 B  52350976 B
v6      779    8      19.16s  40.66        177733632 B  1069056 B
```

`stmts/tick` from the v6 perf logs was 54.03 in both runs. The v5 leg has no
comparable served-engine statement trace in this invocation.

## Gaps

- The v5 row is a v5 `scan` fact at `HEAD`. The v6 row is a file-set row plus a
  successful v6 extraction command over the selected file.
- v5 expands `[[org]]` and joins `repo(r, _, _)` inside the program. v6 has no
  org fan-out spelling; the script performs one shell-level repo loop and
  sends host demands for each repo.
- v5 resolves `HEAD` files from each Git tree and uses the Git revision identity
  in the returned row. The v6 files host reads the working-tree path list
  and hashes each working-tree file with `git hash-object`.
- The v5 glob is one globset expression, `**/*.{go,ts,tsx}`. The v6 leg sends
  three Git pathspec demands, `**/*.go`, `**/*.ts`, and `**/*.tsx`, and unions
  their rows in the v6 relation.
- v5 stores the scan relation in the v5 SQLite schema. v6 opens one served
  SQLite database per selected repo and the reported database size is the sum
  of those scratch files.
- v6 runs `cst`, `type`, `call`, and `df` extraction families. The parity
  number counts one successful extraction row per file; it does not count the
  extracted family facts.
- v6 reports `stmts/tick` from `DL_PERF_LOG`. The v5 one-shot invocation has no
  matching v6 tick trace column.
- The v5 table row covers 389 repos. The v6 table row covers the default cap of
  8 usable repos, so its repo and file counts have different corpus scope.

## Skips and validation

Skip counts for each final run:

```text
discovered repos                 389
missing or no HEAD                 0
git enumeration failures          0
no matching go/ts/tsx files      139
v6 cap-excluded                  242
```

The final script-version runs exited 0. Their total wall times were 38.56s
and 38.50s. `bash -n v6/tsv2/scripts/crawl-bench.sh` and ShellCheck both
completed without diagnostics.
