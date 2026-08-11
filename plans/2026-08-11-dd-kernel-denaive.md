# dd-runner kernel de-naive: the rust x rust arm

Base sha `a65ca7e7`. Target file `v6/dd-runner/src/kernel.rs`, 215 lines at base.
Speed only. Every byte the kernel prints stays the byte it printed at base.

## TOC

| section | what it holds |
|---|---|
| [Measurement rig](#measurement-rig) | what is timed, on what input, with what binary |
| [Baseline](#baseline) | numbers on the untouched tree |
| [Pre-existing failures found at base](#pre-existing-failures-found-at-base) | three, none of them mine |
| [Dependency decision](#dependency-decision) | build-vs-buy, candidate by candidate |
| [Defect 3: linear membership](#defect-3-linear-membership) | `kernel.rs:109-112` at base |
| [Defect 2: full cross product](#defect-2-full-cross-product) | `kernel.rs:160-177` at base |
| [Defect 1: clone and rederive](#defect-1-clone-and-rederive) | `kernel.rs:86-107` at base |
| [Gate output](#gate-output) | verbatim |
| [Closing table](#closing-table) | total speedup per plan |
| [What this lane did not touch](#what-this-lane-did-not-touch) | the 31-plan finding |

## Measurement rig

| item | value |
|---|---|
| binary | `v6/dd-runner/target/release/dd-runner` |
| timer | wall clock around the process, best of 7 |
| RSS | `/usr/bin/time -l` maximum resident set size |
| identity net | stdout sha256 per plan, 215 plans, before against after |
| grade fixtures | the 3 in `grade.sh` |
| scaling plans | 9 synthetic `dd_plan` JSON in the kernel's own input contract |

Process floor is about 20 ms: the binary links bundled SQLite for the other arm,
so any plan whose kernel work is under a millisecond reads as 20 ms.

The three `grade.sh` fixtures carry 3 to 6 rows each, so their wall time is
process startup and nothing else. The synthetic plans exist so a change to the
kernel is visible at all: transitive closure over a line graph (recursion, join,
fixed point), an equijoin with no recursion, and a grouped sum.

The 203 emitted fixture plans in the identity net came from `6_emit_dd_plan.pl`
over the conformance fixtures at this same base sha. 189 of them carry
operators and so exercise the kernel; 17 carry none and run the SQLite arm.

## Baseline

Fixtures, release binary, best of 5:

| fixture | byte-clean | wall ms | peak RSS KB |
|---|---|---:|---:|
| retraction_only_tick_retracts_level_view | yes | 22 | 1824 |
| float_exact_join_has_no_epsilon | yes | 20 | 1808 |
| float_avg_is_grouped | NO, runtime error | 20 | 1792 |

Byte-clean fixture count at base: **2 of 3**. Still 2 of 3 after all three
defects, with the same two fixtures.

Synthetic plans, release binary, best of 7:

| plan | shape | wall ms | peak RSS KB |
|---|---|---:|---:|
| tc_line_24 | closure over 23 edges, 4 ticks | 124 | 8608 |
| tc_line_34 | closure over 33 edges, 4 ticks | 511 | 29408 |
| tc_line_80 | closure over 79 edges, 4 ticks | 25202 | 282576 |
| join_300 | 300 x 300 rows, 75 keys, 4 ticks | 401 | 92560 |
| join_700 | 700 x 700 rows, 175 keys, 4 ticks | 2175 | 417856 |
| join_1400 | 1400 x 1400 rows, 350 keys, 4 ticks | 9190 | 1550880 |
| reduce_600 | 600 rows, 12 groups, 4 ticks | 32 | 2768 |
| reduce_2000 | 2000 rows, 12 groups, 4 ticks | 129 | 7136 |
| reduce_20000 | 20000 rows, 12 groups, 4 ticks | 9505 | 52992 |

Three of those cross the 10-second law at base: a line graph of 80 nodes takes
25.2 s, a 1400-row join 9.2 s at 1.5 GB resident, a 20000-row group 9.5 s. None
of the three does now.

Wide corpus at base, kernel arm: 82 clean, 88 byte-diff, 33 runtime error over
203 plans. That corpus is the identity net, not a target; the 121 non-clean
plans have to keep printing exactly the bytes they print now, and they do.

## Pre-existing failures found at base

| what | receipt | owner |
|---|---|---|
| `grade.sh` never reaches its own fixtures | its first line runs plunit, and `catalog_plane_rail:level_plane_family_corpus_counts` fails on `a65ca7e7` under `set -e` | `v6/prolog`, not this lane |
| `cargo clippy -- -D warnings` fails | `src/main.rs:113` `clippy::manual_repeat_n`, `std::iter::repeat("?").take(n)` | `main.rs`, another lane's file |
| fixture 3 errors out | `dd-runner: projection source missing: group`; the reduce group key is spelled `b0.group` in `bindings` and `group` in `aggregate.group` | see the closing section |

Fixture grading here runs `grade.sh`'s `grade()` body with the plunit line
removed, so the fixture verdicts are the verdicts `grade.sh` would print.

## Dependency decision

Outcome: **no new dependency**, and one hand-rolled piece deleted in favour of a
dependency that already ships in the tree.

The kernel's three defects are two different shapes. Defects 2 and 3 are "index
a Vec instead of scanning it", which `std` answers exactly. Defect 1 is
"incremental fixed point", which is the shape a dataflow library is built for.
Candidates for each, with what each would cost here.

| candidate | version on crates.io | what it would do here | why not |
|---|---|---|---|
| `differential-dataflow` + `timely` | 0.25.1 / 0.31.0 | the whole kernel: arrangements, signed weights, timestamps, consolidation. It is the thing this arm is named after | The plan arrives at runtime as JSON with runtime arity and runtime binding order, so every collection is `Vec<Value>` and every join key is chosen at runtime. Output has to be a per-tick add/del log in `serde_json::to_string` sort order, so dd's arranged batches would be re-sorted on the way out. It also brings a worker/scheduler runtime into a binary whose whole job is being the cheap reference. This is the right library for a rewrite of the arm, not for removing three naive shapes from a 215-line evaluator, and that rewrite is `plans/2026-08-10-dd-dance-recon.PLAN.md:136-141`'s 260-360 line kernel, a separate arc |
| `datafrog` | 2.0.1 | the fixed point: `Variable` + `from_join`, semi-naive, no runtime, no deps | Its relations are sorted vectors and every operation needs `Ord` on the tuple. `serde_json::Value` implements no `Ord` (compile receipt below), so every tuple would need a total order invented for it, and inventing one changes nothing about output bytes while adding a sort per round. It also has no aggregation, and 34 of the 189 kernel plans carry a reduce operator, so the reduce path would stay hand-written next to it |
| `ascent` / `crepe` | 0.8.0 / 0.2.0 | datalog with the rules written in Rust | Rules are data here, not source. Both want the program at compile time |
| `indexmap` | 2.14.0 | insertion-ordered set with O(1) membership, which is exactly `Relation` | It is a real fit and the only close call. Rejected because `Relation` also needs its rows as `Rc<Tuple>` shared with the previous round for the pointer-equality round compare, which is a second index over the same rows; `Vec<Rc<Tuple>>` plus `HashSet<RowKey>` is 14 lines and does both. A dependency that covers one of the two jobs is not worth the wire |
| `serde-hashkey` | 0.4.5 | a hashable key type for a serde value | Not needed, see the next row |
| `serde_json`'s own `Hash` | 1.0.151, already a dependency | hash `Value` directly | This is what shipped. Measured, not assumed: `Hash` is implemented for `Value`, `hash(0.0) == hash(-0.0)` is true, and a `HashSet` built on `0.0` finds `-0.0`, which is what its `Eq` requires since `Value` compares f64 with `==`. Commit `f20d07c9` deletes 45 lines of hand-written tree hashing that had solved this already-solved problem |

Compile receipts, from a probe crate on the same `serde_json 1.0.151` this crate
pins:

```
error: the trait `Ord` is not implemented for `Value`
```
```
0.0 == -0.0 : true   texts 0.0 -0.0
hash(0.0) == hash(-0.0) : true
HashSet built on 0.0 contains -0.0 : true
1 == 1.0 : false   hash equal : false
```

The second block is why the membership key is the value tree and never its
serialized text: `0.0` and `-0.0` are one value and two strings.

## Defect 3: linear membership

Commit `79f10aff`.

Was: `insert_rows` (`kernel.rs:109-112`), `change` (`:78-84`) and `tick_json`
(`:197-210`) all decided membership with `Vec::contains`, a linear scan of
`Vec<Value>` comparisons per candidate row.

Is: `Relation` is an insertion-ordered set, `Vec<Rc<Tuple>>` for order plus a
`HashSet<RowKey>` for the probe. Row order is kept exactly, because
`eval_reduce` folds f64 in row order and f64 addition is not associative.

| plan | base ms | after ms | ratio |
|---|---:|---:|---:|
| tc_line_24 | 124 | 118 | 1.05x |
| tc_line_34 | 511 | 487 | 1.05x |
| tc_line_80 | 25202 | 21802 | 1.16x |
| join_300 | 401 | 377 | 1.06x |
| join_700 | 2175 | 2014 | 1.08x |
| join_1400 | 9190 | 8713 | 1.05x |
| reduce_600 | 32 | 22 | 1.45x |
| reduce_2000 | 129 | 28 | 4.61x |
| reduce_20000 | 9505 | 138 | 68.9x |

The reduce column is where linear membership actually lived: `tick_json` diffed
a 20000-row relation against itself row by row. The join and closure columns
barely move because their cost is the cross product, which is defect 2.

## Defect 2: full cross product

Commit `6cce1ddb`.

Was: `binding_rows` (`kernel.rs:160-177`) built the full product of every bound
relation as `Vec<BTreeMap<String, Value>>`, one allocated `alias.column` key per
column per row, and applied predicates only afterwards.

Is: `Query::compile` resolves each binding once. An equality predicate with one
side already bound and the other a column of the arriving relation becomes an
equijoin: the arriving side is indexed on that key and probed. A literal
equality on the arriving relation filters its tuples before the product. What is
left is still a nested loop, which is what a plan with no connecting predicate
means. A bound row became a slot vector; `None` in a slot is exactly what an
absent map key meant, so a predicate over two absent columns still holds and a
projection over an absent one still fails with the same text.

| plan | after defect 3 ms | after defect 2 ms | ratio | RSS KB, 3 -> 2 |
|---|---:|---:|---:|---|
| tc_line_24 | 118 | 27 | 4.37x | 12656 -> 2416 |
| tc_line_34 | 487 | 38 | 12.8x | 28624 -> 2800 |
| tc_line_80 | 21802 | 310 | 70.3x | 294416 -> 9328 |
| join_300 | 377 | 27 | 14.0x | 91024 -> 3376 |
| join_700 | 2014 | 33 | 61.0x | 417104 -> 7536 |
| join_1400 | 8713 | 46 | 189x | 1535744 -> 8304 |
| reduce_600 | 22 | 23 | 0.96x | 2608 -> 2320 |
| reduce_2000 | 28 | 26 | 1.08x | 4496 -> 3424 |
| reduce_20000 | 138 | 72 | 1.92x | 51120 -> 31808 |

## Defect 1: clone and rederive

Commit `a4c5aea3`.

Was: `settle` (`kernel.rs:86-107`) cloned the whole state twice per round and
re-derived every operator from base, up to 10000 rounds.

Is: each round still builds the same state it built before, byte for byte; what
changed is the work to reach it. Per relation a round records `Same`,
`Appended(prefix)` or `Rebuilt`. An operator whose bindings are all `Same`
reuses last round's rows. A map operator whose only moved binding is the
outermost one, moved by appending, is evaluated over that binding's tail alone
and its rows extend the ones it already produced: the fork 1 shape from
`v6/sprefa-store/src/engine.rs:642`, only the `+1` crossings propagate. Anything
else is a full re-derive, as before. The per-round `state.clone()` is gone.

Appending is restricted to the outermost binding on purpose. The nested loop is
lexicographic, so growth in an inner binding interleaves new rows among old ones
and the produced order stops being an extension of the previous one. Row order
is observable through `eval_reduce`'s f64 fold, so that restriction is what
keeps the bytes.

| plan | after defect 2 ms | after defect 1 ms | ratio |
|---|---:|---:|---:|
| tc_line_24 | 27 | 22 | 1.23x |
| tc_line_34 | 38 | 25 | 1.52x |
| tc_line_80 | 310 | 80 | 3.88x |
| join_300 | 27 | 24 | 1.13x |
| join_700 | 33 | 28 | 1.18x |
| join_1400 | 46 | 37 | 1.24x |
| reduce_600 | 23 | 21 | 1.10x |
| reduce_2000 | 26 | 23 | 1.13x |
| reduce_20000 | 72 | 52 | 1.38x |

Six of the nine plans are now inside 5 ms of the 20 ms process floor, so those
ratios are floor-bound and understate the change. `tc_line_80`, the only plan
with real work left, is the accurate one at 3.88x. Its peak RSS also falls from
9328 KB to 3792 KB, which is the per-round state clone going away.

## Gate output

```
=== cargo build --release
warning: `dd-runner` (bin "dd-runner") generated 1 warning
    Finished `release` profile [optimized] target(s) in 0.15s
exit=0
=== ./grade.sh
.. passed (1.558 sec)
ERROR: [Thread main] /Users/chrishafley/projects/sprefa/.claude/worktrees/agent-a99c43d1e092d630d/v6/prolog/compile/test/plunit_tests.pl:1312:
ERROR: [Thread main]     test catalog_plane_rail:level_plane_family_corpus_counts: failed
ERROR: [Thread main] 1 test failed
exit=1
=== cargo clippy --all-targets -- -D warnings
error: could not compile `dd-runner` (bin "dd-runner") due to 1 previous error
warning: build failed, waiting for other jobs to finish...
error: could not compile `dd-runner` (bin "dd-runner" test) due to 1 previous error
=== cargo fmt --check
   1 src/main.rs:107:
   1 src/main.rs:118:
   1 src/main.rs:126:
   1 src/main.rs:143:
   1 src/main.rs:173:
```

Reading of those four:

| gate | verdict | note |
|---|---|---|
| `cargo build --release` | green | |
| `./grade.sh` | red at base and red now, same line | plunit `catalog_plane_rail:level_plane_family_corpus_counts`. Its fixture half, run without the plunit line, prints `retraction_only_tick_retracts_level_view: byte-diff clean` and `float_exact_join_has_no_epsilon: byte-diff clean`, the same 2 of 3 as base |
| `cargo clippy` | red at base and red now, same line | `src/main.rs:113`, `clippy::manual_repeat_n`. `kernel.rs` produces zero clippy findings |
| `cargo fmt --check` | red at base, less red now | 24 diffs at base (19 in `kernel.rs`, 5 in `main.rs`), 5 now, all in `main.rs`. `kernel.rs` is rustfmt-clean |

`main.rs` belongs to another lane, so both red gates stay red until that lane
lands.

## Closing table

Base sha `a65ca7e7` against `f20d07c9`, release binary, best of 7, same machine,
same rig.

| plan | base ms | now ms | speedup | base RSS KB | now RSS KB | RSS ratio |
|---|---:|---:|---:|---:|---:|---:|
| tc_line_24 | 124 | 22 | 5.6x | 8608 | 2048 | 4.2x |
| tc_line_34 | 511 | 25 | 20.4x | 29408 | 2256 | 13.0x |
| tc_line_80 | 25202 | 87 | 290x | 282576 | 5472 | 51.6x |
| join_300 | 401 | 23 | 17.4x | 92560 | 2944 | 31.4x |
| join_700 | 2175 | 28 | 77.7x | 417856 | 4416 | 94.6x |
| join_1400 | 9190 | 36 | 255x | 1550880 | 6768 | 229x |
| reduce_600 | 32 | 21 | 1.5x | 2768 | 2336 | 1.2x |
| reduce_2000 | 129 | 23 | 5.6x | 7136 | 3376 | 2.1x |
| reduce_20000 | 9505 | 51 | 186x | 52992 | 21520 | 2.5x |
| retraction_only_tick_retracts_level_view | 22 | 21 | 1.0x | 1824 | 1888 | 1.0x |
| float_exact_join_has_no_epsilon | 20 | 21 | 1.0x | 1808 | 1856 | 1.0x |
| float_avg_is_grouped | 20 | 20 | 1.0x | 1792 | 1840 | 1.0x |

The three `grade.sh` fixtures do not move, and cannot: they carry 3 to 6 rows
and their whole runtime is process startup.

Ratio to the SQLite arm: `grade.sh` gives none. It runs each fixture through one
arm only, and at fixture scale both arms sit on the 20 ms process floor, so a
ratio taken there would be measuring `dlopen`. The synthetic plans carry no SQL
bundle, so the SQLite arm cannot run them at all. A real two-arm ratio needs
plans that are both emitted and large, which is `ARCH.pl:873` `oracle_scale_ceiling`,
still marked "User call".

## What this lane did not touch

`float_avg_is_grouped` fails at base with `projection source missing: group`.
The plan spells the same column two ways: `bindings` produce `b0.group`, and
`aggregate.group` carries the bare `group`. `Query::column` resolves names
against the aliased slots, so the bare name misses.

Measured across the corpus: **31 of the 33 runtime-error plans are that one
shape**, all of them a bare name in `aggregate.group`. Every one of those 31
currently prints an error line, so teaching the kernel to resolve a bare
aggregate column would move bytes on 31 plans at once. That is a semantics
change on a plan-format disagreement, not a speed change, and the fix may belong
in `6_emit_dd_plan.pl` rather than here. It would take `grade.sh` from 2 of 3 to
3 of 3. Left for a lane that owns both sides.
