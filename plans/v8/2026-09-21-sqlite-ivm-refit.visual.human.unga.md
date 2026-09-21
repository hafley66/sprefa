# sqlite_ivm refit, unga version

the library dl8 loads to run its sqlite engine was built from an old copy of
the sqlite_ivm source. we rebuilt it from that repo's current main, then ran
the tests twice: once with the old file, once with the new one. same machine,
same commands, logging off, three runs each.

## the two files

| | old | new |
|---|---|---|
| where | `~/projects/sqlite_ivm/target/release/libsqlite_ivm.dylib` | `$CARGO_TARGET_DIR/release/libsqlite_ivm.dylib` |
| bytes | 2839072 | 3112304 |
| sha256 starts | `606e1a3e` | `f0b808ca` |
| source | sqlite_ivm `2950cb6`, built in a lane checkout | sqlite_ivm `68a8832`, that repo's main |
| how far apart | 94 commits, 15 pull requests | |

the old file carries no note of its commit. the compiler's dependency file next
to it does list the source paths, and the checkout it points at sits at a commit
whose content is byte for byte `2950cb6`. so the name is known, not guessed.

## what dl8 asks the library for

```
one program
  |
  +-- declare ....... say the relation tables and one view
  |                    CREATE VIRTUAL TABLE ... USING sqlite_ivm('SELECT ...')
  +-- apply ......... put the seed rows in the source tables
  +-- read .......... read the view

comptime does those three again on every round.
```

whatever got slower inside the library shows up as comptime getting slower.

## the big row: whole test battery, seconds

| engine | old file | new file | verdict |
|---|---|---|---|
| rust (the default) | 66.19 65.51 61.53 | 121.21 119.18 119.93 | slower, 1.83x |
| sqlite | 421.68 444.52 401.70 | 733.12 730.04 735.56 | slower, 1.74x |

not one after-run lands inside the before spread on either row. the rust row
moves because two test files inside it run the sqlite engine.

## comptime on real programs, seconds

| what | engine | old file | new file |
|---|---|---|---|
| biggest comptime case, 1.8 MB | rust | 0.16 | 0.14 |
| biggest comptime case, 1.8 MB | sqlite | 3.08 2.88 2.89 | 8.02 8.00 8.11 |
| compile `fixtures/openapi/todo.dl7` | rust | 0.19 0.15 0.15 | 0.14 0.14 0.14 |
| compile `fixtures/openapi/todo.dl7` | sqlite | 2.91 3.79 4.02 | 4.72 4.75 4.72 |

and the memory it holds, one process, megabytes:

| what | engine | old file | new file |
|---|---|---|---|
| biggest comptime case | sqlite | 792 939 771 | 1440 1438 1540 |
| compile `fixtures/openapi/todo.dl7` | sqlite | 755 536 355 | 1256 1257 1229 |
| compile `fixtures/openapi/todo.dl7` | rust | 83 80 80 | 83 81 83 |

## where the rust battery time went

seconds, one run each side.

| test file | old | new | delta |
|---|---|---|---|
| the library's own unit tests | 22.20 | 58.24 | +36.0 |
| `_27_eval_sqlite` | 11.90 | 30.04 | +18.1 |
| every other rust file | 32 | 32 | flat |

those two files are the whole +55 s.

## the cell that moved

one `dl8 compile fixtures/openapi/todo.dl7`. same 1275 sql statements both
files, same counts by name. the milliseconds differ:

| statement | old ms | new ms |
|---|---|---|
| declare the view | 1427.9 | 3168.7 |
| flush term args | 600.0 | 642.2 |
| flush terms | 267.3 | 277.5 |
| nonlinear round | 215.8 | 235.6 |
| first seed insert | 19.3 | 178.5 |
| declare tables | 12.4 | 12.6 |

declaring one view costs 2.2x more. the first seed insert costs 9x more.
dl8 declares a fresh view for every program and again for every comptime round.
that is the whole story.

the engine work in those 15 pull requests is maintenance work: no mutation
rebuilds the view, drains go set at a time, one prepared statement program per
view, recursion deletes and rederives. dl8 never mutates a view it declared. it
drops the view and declares a new one. so the engine work paid off where dl8
does not reach.

## did anything break

no. the same test files fail with both files.

| test file | rust engine | sqlite engine |
|---|---|---|
| `_16_extract_tsi` | red before, red after | red before, red after |
| `_22_book` | red before, red after | red before, red after |
| `_15_fold` | green | red before, red after |

`_15_fold` is green on the rust engine and red on the sqlite engine with both
files. it returns no rows: `1_program_step: rows differ, got: []`. that is a dl8
gap in the sqlite path, older than this work, and this lane did not touch it.

the oracle comparison through the binary reads `sqlite oracles 32/32` with both
files. that is every eval fixture the repo froze.

## the check that asks whether the cached view lies

`DL8_EVAL_CHECK=1`, the sqlite engine, three cells: no warning at all, both
files. the built-in rail that compares a staged insert against a fresh view over
all 32 oracle programs passes with both files too.

## so

comptime got slower. on the sqlite engine the whole battery is 1.74x slower and
the biggest comptime case is 2.78x slower. the default engine's battery is 1.83x
slower for the same reason. the rust engine by itself, on the same programs,
does not move.

and the sqlite engine stays the expensive one either way: compiling
`fixtures/openapi/todo.dl7` costs 25x the rust engine with the old file and 34x
with the new one. the biggest comptime case costs 18x with the old file and 57x
with the new one.

| ratio against the rust engine, same cell | old file | new file |
|---|---|---|
| whole battery | 6.4x | 6.1x |
| compile `fixtures/openapi/todo.dl7` | 25x | 34x |
| biggest comptime case | 18x | 57x |

keep the default on rust. the new library is the right one to ship for
correctness, but it is not faster for the way dl8 uses it.