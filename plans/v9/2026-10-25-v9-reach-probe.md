# v9 reach probe: skeleton vs Core (P6)

Fixture: `~/projects/hafley-rs/crates/hafley_scm` (the #820 corpus). Binaries: `--release`, built
from `v9` at `1e8c95df9` (skeleton) and from branch `v9-p6` (Core). Machine: darwin arm64, no
other cargo running.

## Commands

```sh
cd ~/projects/sprefa-wt/v9-p6/v9
RCARGO_OFF=1 cargo build --release --bin v9   # binary: target/rcargo/aarch64-apple-darwin/release/v9
```

`measure.sh BIN TAG` (scratch script): three cold runs, each on a new db, then three warm reruns on
the third cold db; each run is

```sh
/usr/bin/time -l BIN reach ~/projects/hafley-rs/crates/hafley_scm DB --check
```

`wall` is `real`, `peak MB` is `maximum resident set size` / 2^20. The skeleton and Core series
ran back to back.

Row equality between the two dbs (third cold run of each):

```sh
sqlite3 core-cold3.db "ATTACH 'skel-cold3.db' AS s;
  SELECT count(*) FROM (SELECT CAST(caller AS TEXT), CAST(callee AS TEXT) FROM main.reach_text
    EXCEPT SELECT CAST(caller AS TEXT), CAST(callee AS TEXT) FROM s.reach_text);
  SELECT count(*) FROM (SELECT CAST(caller AS TEXT), CAST(callee AS TEXT) FROM s.reach_text
    EXCEPT SELECT CAST(caller AS TEXT), CAST(callee AS TEXT) FROM main.reach_text);
  SELECT count(*) FROM (SELECT caller, callee FROM main.edge EXCEPT SELECT caller, callee FROM s.edge);
  SELECT count(*) FROM (SELECT caller, callee FROM s.edge EXCEPT SELECT caller, callee FROM main.edge);"
```

| db | edge rows | reach rows | reach text pairs only here | edge id pairs only here |
| --- | --- | --- | --- | --- |
| skeleton | 4737 | 24322 | 0 | 0 |
| Core | 4737 | 24322 | 0 | 0 |

## Skeleton (`v9` at `1e8c95df9`)

Printed line: `ms load L ryi R replay P delta D write W`.

| run | wall ms | peak MB | ryi ms | replay ms | delta ms | write ms | edges | reach | rows written | check |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| cold 1 | 3010 | 34.2 | 2948 | 0 | 17 | 24 | 4737 (+4737) | 24322 (+24322) | 31839 | equal |
| cold 2 | 700 | 34.1 | 651 | 0 | 16 | 25 | 4737 (+4737) | 24322 (+24322) | 31839 | equal |
| cold 3 | 750 | 34.1 | 702 | 0 | 16 | 25 | 4737 (+4737) | 24322 (+24322) | 31839 | equal |
| warm 1 | 690 | 34.8 | 667 | 16 | 0 | 0 | 4737 (+0 -0) | 24322 (+0 -0) | 0 | equal |
| warm 2 | 650 | 34.8 | 626 | 16 | 0 | 0 | 4737 (+0 -0) | 24322 (+0 -0) | 0 | equal |
| warm 3 | 690 | 34.9 | 668 | 16 | 0 | 0 | 4737 (+0 -0) | 24322 (+0 -0) | 0 | equal |

## Core (`v9-p6`)

Printed line: `ms open O ryi R step+write S`. `open` is `build` (store replay and the engine's
first step); `ryi` is the time inside the ryi service's `call`; `step+write` is the commit minus
`ryi`.

| run | wall ms | peak MB | open ms | ryi ms | step+write ms | edges | reach | rows written | check |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| cold 1 | 800 | 48.9 | 3 | 642 | 109 | 4737 (+4737) | 24322 (+24322) | 36579 | equal |
| cold 2 | 820 | 47.0 | 1 | 665 | 108 | 4737 (+4737) | 24322 (+24322) | 36579 | equal |
| cold 3 | 790 | 47.1 | 1 | 637 | 108 | 4737 (+4737) | 24322 (+24322) | 36579 | equal |
| warm 1 | 790 | 58.6 | 58 | 683 | 3 | 4737 (+0 -0) | 24322 (+0 -0) | 0 | equal |
| warm 2 | 770 | 58.4 | 59 | 659 | 3 | 4737 (+0 -0) | 24322 (+0 -0) | 0 | equal |
| warm 3 | 790 | 58.4 | 59 | 678 | 3 | 4737 (+0 -0) | 24322 (+0 -0) | 0 | equal |

The skeleton's cold 1 `ryi` is 2948 ms; in an earlier series Core's cold 1 `ryi` was 2946 ms and
the skeleton's 3007 ms. The first `ryi fast .` after an idle gap takes about 3 s in either binary.

Rows written: skeleton 31839 = dict 2780 + edge 4737 + reach 24322. Core 36579 = dict 2781 +
root 1 + ryi_ask 1 + ryi_edge 4737 + edge 4737 + reach 24322.

## Phases

### Time: step vs write

Core's `step+write` is one number in the bin. A scratch harness (`phase`, outside the repo)
replays `core-cold3.db` (root and ryi_edge rows) into a fresh `Dd` under the `reach` plan, times
the one `step`, and with `write` writes that delta plus the whole dict to a new `Sqlite`:

```sh
phase core-cold3.db phase-out.db step    # x3
phase core-cold3.db phase-out.db write   # x3
```

| mode | changes | step ms | write ms | peak MB |
| --- | --- | --- | --- | --- |
| step | 33798 | 60 | | 31.2 |
| step | 33798 | 58 | | 30.1 |
| step | 33798 | 58 | | 28.8 |
| write | 33798 | 58 | 39 | 34.8 |
| write | 33798 | 59 | 37 | 32.2 |
| write | 33798 | 58 | 37 | 31.5 |

| phase | skeleton ms | Core ms |
| --- | --- | --- |
| engine step, full input | 16 (delta cold, replay warm) | 58 to 60 (harness step; warm `open`) |
| store write, cold | 24 to 25 | 37 to 39 (harness write) |
| ryi | 626 to 702 | 637 to 683 |

### Memory

Peak MB without `--check`, Core, three runs each: cold 39.2, 38.6, 39.2; warm 60.0, 58.8, 60.0.

Resident KB (`ps -o rss`) at points of one cold run and one warm run, from a temporary build of
the bin (not committed):

| point | cold KB | warm KB |
| --- | --- | --- |
| after `build` (open) | 5312 | 28384 |
| after seeding `answered` | 5360 | 28432 |
| after the ryi call for a stored root | 5392 | 37536 |
| after `commit` | 38880 | 37760 |

`ryi fast .` on the fixture writes 22782159 bytes to stdout (`ryi fast . | wc -c`), held whole by
`Command::output` during the ryi call. On a cold Core run the call happens inside `commit` before
the engine holds rows; on a warm Core run it happens after `open` has replayed and stepped the
stored rows.

| phase | skeleton peak MB | Core peak MB |
| --- | --- | --- |
| cold, `--check` | 34.1 to 34.2 | 47.0 to 48.9 |
| cold, no `--check` | | 38.6 to 39.2 |
| warm, `--check` | 34.8 to 34.9 | 58.4 to 58.6 |
| warm, no `--check` | | 58.8 to 60.0 |
