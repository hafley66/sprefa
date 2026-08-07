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

The v6 program is generated in the scratch directory. It declares the
repo-scoped file host from `v6/dl/fixtures/files-hosts.dl6` and a repo-scoped
extraction host:

```text
repo_files(repo, glob)          -> git -C <repo> ls-files + git hash-object
repo_extract(repo, path, digest) -> sprefa-extract --family cst,type,call,df <repo>/<path>
```

**The shell loop over repositories is gone (2026-07-31).** It was the
`SLOT-ORG-FANOUT` gap the first version of this page recorded: the v6 program
had no way to say which repository a file came from, so the script supplied
that operation at the orchestration boundary with one served process, one
sqlite database and one program load PER REPOSITORY.

Ruling `repo_column_spelling = distinct_name_hosts` closed it. The repository
root is an ordinary demand column on distinct-named hosts, so the leg now
starts ONE server, loads ONE program, and posts ONE `/edb/events` batch holding
`want_repo(root, glob)` for every selected repository and every glob. Each
fan-out below that -- per repository, then per file -- is rows through the
incremental emitter.

The extraction host still answers one success row per extracted file, on
purpose: this bench's question is the loop, so the extraction leg has to stay
the work it was. (Measured while writing it: capturing every `cst`/`type`/
`call`/`df` record as an EDB arrival instead takes the same 779-file corpus
from 20.26s to 62.97s and the scratch database from 1.0MB to 595MB. That is a
real number about the extraction seam, and it is a different question.)

### Before / after, same corpus, same extraction leg

`scripts/crawl-bench-loop-baseline.tsv` pins the pre-change measurement (the
loop no longer exists in the script, so it cannot be re-derived by running it);
`report_loop_delta` prints it beside every run at a matching scope.

| scope | files | before (loop) | after (one program) | speedup |
|---|---:|---:|---:|---:|
| first 8 usable | 779 | 20.26s, 38.45 files/s | 18.08s, 43.09 files/s | 1.12x |
| first 32 usable | 2,890 | 72.19s, 40.03 files/s | 57.55s, 50.22 files/s | 1.25x |

Row counts agreed exactly on both sides (`repo_file` 2,890 / `extracted` 2,890
at cap 32), so the two legs did the same work. The saving is per REPOSITORY and
not per file, which is what the two rows show: ~0.27s/repo at cap 8 and
~0.46s/repo at cap 32, against a per-file extraction cost that did not move.
`stmts/tick` stayed 54.03-54.04 throughout -- the fan-out is rows, not
statements.

Scratch database size went UP (1.0MB -> 1.4MB at cap 8), which is the honest
cost of the change: one database now holds every repository's rows plus the
repo column on each, where before the number was the sum of N smaller files
with no repo column in them.

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

Re-run 2026-07-31 of the whole recipe after the loop was removed
(`bash scripts/crawl-bench.sh --max-repos 8`, exit 0, 38.66s total):

```text
engine  files  repos  wall    files-per-s  RSS          db size
v5      42739  389    12.99s  3290.15      344227840 B  52330496 B
v6        779    8    17.96s    43.37      196591616 B   1368064 B

loop delta (first-8-usable-of-389-cap-8, 779 files):
  before (one served process per repository)  wall=20.26s  38.45 files/s
  after  (ONE program, repo as a column)      wall=17.96s  43.37 files/s
  speedup 1.13x
```

## Gaps

- The v5 row is a v5 `scan` fact at `HEAD`. The v6 row is a file-set row plus a
  successful v6 extraction command over the selected file.
- ~~v5 expands `[[org]]` and joins `repo(r, _, _)` inside the program. v6 has
  no org fan-out spelling; the script performs one shell-level repo loop and
  sends host demands for each repo.~~ CLOSED 2026-07-31. The repo set is a
  column, the loop is gone, and `v6/dl/fixtures/crawl_org.dl6` goes one step
  further than v5 by discovering the repository set itself through a `repos`
  host on an interval bind rather than reading it out of a config file. The
  bench still POSTS its repository set rather than discovering it, because
  `--max-repos` has to select a corpus slice.
- v5 resolves `HEAD` files from each Git tree and uses the Git revision identity
  in the returned row. The v6 files host reads the working-tree path list
  and hashes each working-tree file with `git hash-object`.
- The v5 glob is one globset expression, `**/*.{go,ts,tsx}`. The v6 leg sends
  three Git pathspec demands, `**/*.go`, `**/*.ts`, and `**/*.tsx`, and unions
  their rows in the v6 relation.
- v5 stores the scan relation in the v5 SQLite schema. v6 opens ONE served
  SQLite database for the whole selected corpus (it opened one per repository
  until 2026-07-31).
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
