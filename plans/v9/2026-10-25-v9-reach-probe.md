# v9 reach probe: skeleton vs Core

Fixture: `~/projects/hafley-rs/crates/hafley_scm` (the #820 corpus). Binaries: `--release`. The
skeleton is built from `v9` at `1e8c95df9`. The Core table below is the F2 rerun, built from branch
`v9-f2`; the row equality, phase and memory sections were measured on the P6 Core binary (branch
`v9-p6`). Machine: darwin arm64.

## Commands

From the repo root:

```sh
cd v9
RCARGO_OFF=1 cargo build --release --bin v9   # binary: the path cargo prints under target/
```

`measure.sh BIN DIR` (scratch script, not committed): three cold runs, each on a new db, then three
warm reruns on the third cold db:

```sh
BIN=$1; DIR=$2; ROOT=$HOME/projects/hafley-rs/crates/hafley_scm
run() {
  /usr/bin/time -l "$BIN" reach "$ROOT" "$2" --check > "$DIR/out.txt" 2> "$DIR/time.txt"
  real=$(awk '/ real /{print $1}' "$DIR/time.txt")
  rss=$(awk '/maximum resident set size/{print $1}' "$DIR/time.txt")
  echo "$1 | $real | $rss | $(tr '\n' ' ' < "$DIR/out.txt")"
}
for i in 1 2 3; do rm -f "$DIR/cold$i.db"; run "cold $i" "$DIR/cold$i.db"; done
for i in 1 2 3; do run "warm $i" "$DIR/cold3.db"; done
```

`wall` is `real` (10 ms resolution), `peak MB` is `maximum resident set size` / 2^20.

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

Measured on the P6 Core binary.

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

## Core (`v9-f2`)

Printed line: `ms open O commit C refresh R` (`src/bin/v9.rs`). `open` is `build` (store replay and
the engine's first step). `commit` is `Core::commit` of the root changes: on a cold run it holds the
ryi call, the engine step and the store write. `refresh` is `Core::refresh("ryi")` on a stored
root: on a warm run it holds the ryi call.

| run | wall ms | peak MB | open ms | commit ms | refresh ms | edges | reach | rows written | check |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| cold 1 | 3410 | 50.2 | 10 | 3345 | 0 | 4737 (+4737 -0) | 24322 (+24322 -0) | 36579 | equal |
| cold 2 | 920 | 52.7 | 1 | 868 | 0 | 4737 (+4737 -0) | 24322 (+24322 -0) | 36579 | equal |
| cold 3 | 940 | 51.1 | 1 | 887 | 0 | 4737 (+4737 -0) | 24322 (+24322 -0) | 36579 | equal |
| warm 1 | 870 | 62.1 | 62 | 0 | 757 | 4737 (+0 -0) | 24322 (+0 -0) | 0 | equal |
| warm 2 | 850 | 62.1 | 62 | 0 | 745 | 4737 (+0 -0) | 24322 (+0 -0) | 0 | equal |
| warm 3 | 860 | 59.4 | 63 | 0 | 744 | 4737 (+0 -0) | 24322 (+0 -0) | 0 | equal |

### Core (`v9-p6`), the earlier line

Printed line: `ms open O ryi R step+write S`. `ryi` is the time inside the ryi service's `call`;
`step+write` is the commit minus `ryi`.

| run | wall ms | peak MB | open ms | ryi ms | step+write ms | edges | reach | rows written | check |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| cold 1 | 800 | 48.9 | 3 | 642 | 109 | 4737 (+4737) | 24322 (+24322) | 36579 | equal |
| cold 2 | 820 | 47.0 | 1 | 665 | 108 | 4737 (+4737) | 24322 (+24322) | 36579 | equal |
| cold 3 | 790 | 47.1 | 1 | 637 | 108 | 4737 (+4737) | 24322 (+24322) | 36579 | equal |
| warm 1 | 790 | 58.6 | 58 | 683 | 3 | 4737 (+0 -0) | 24322 (+0 -0) | 0 | equal |
| warm 2 | 770 | 58.4 | 59 | 659 | 3 | 4737 (+0 -0) | 24322 (+0 -0) | 0 | equal |
| warm 3 | 790 | 58.4 | 59 | 678 | 3 | 4737 (+0 -0) | 24322 (+0 -0) | 0 | equal |

The skeleton's cold 1 `ryi` is 2948 ms; in an earlier series Core's cold 1 `ryi` was 2946 ms and
the skeleton's 3007 ms. The F2 cold 1 `commit` is 3345 ms against 868 and 887 for cold 2 and 3.
The first `ryi fast .` after an idle gap takes about 3 s in either binary.

Rows written: skeleton 31839 = dict 2780 + edge 4737 + reach 24322. Core 36579 = dict 2781 +
root 1 + ryi_ask 1 + ryi_edge 4737 + edge 4737 + reach 24322.

## Phases (P6 binary)

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
