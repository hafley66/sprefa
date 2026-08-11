# dd-runner kernel de-naive: the rust x rust arm

Base sha `a65ca7e7`. Target file `v6/dd-runner/src/kernel.rs`, 215 lines at base.
Speed only. Every byte the kernel prints stays the byte it printed at base.

## TOC

| section | what it holds |
|---|---|
| [Measurement rig](#measurement-rig) | what is timed, on what input, with what binary |
| [Baseline](#baseline) | numbers on the untouched tree |
| [Pre-existing failures found at base](#pre-existing-failures-found-at-base) | two, neither mine |
| [Dependency decision](#dependency-decision) | build-vs-buy, candidate by candidate |
| [Defect 3: linear membership](#defect-3-linear-membership) | `kernel.rs:109-112` |
| [Defect 2: full cross product](#defect-2-full-cross-product) | `kernel.rs:160-177` |
| [Defect 1: clone and rederive](#defect-1-clone-and-rederive) | `kernel.rs:86-107` |
| [Gate output](#gate-output) | verbatim |
| [Closing table](#closing-table) | total speedup per plan |

## Measurement rig

| item | value |
|---|---|
| binary | `v6/dd-runner/target/release/dd-runner` |
| timer | wall clock around the process, best of 3 |
| RSS | `/usr/bin/time -l` maximum resident set size |
| identity net | stdout sha256 per plan, 212 plans, before vs after |
| grade fixtures | the 3 in `grade.sh` |
| scaling plans | 6 synthetic `dd_plan` JSON in the kernel's own input contract |

The three `grade.sh` fixtures carry 3 to 6 rows each, so their wall time is
process startup and nothing else. The synthetic plans exist so a change to the
kernel is visible at all: a transitive closure over a line graph (recursion,
join, fixed point), an equijoin with no recursion, and a grouped sum.

## Baseline

Fixtures, release binary, best of 5:

| fixture | byte-clean | wall ms | peak RSS KB |
|---|---|---:|---:|
| retraction_only_tick_retracts_level_view | yes | 22 | 1824 |
| float_exact_join_has_no_epsilon | yes | 20 | 1808 |
| float_avg_is_grouped | NO, runtime error | 20 | 1792 |

Byte-clean fixture count at base: **2 of 3**.

Synthetic plans, release binary, best of 3:

| plan | shape | wall ms | peak RSS KB |
|---|---|---:|---:|
| tc_line_24 | closure over 23 edges, 4 ticks | 133 | 15264 |
| tc_line_34 | closure over 33 edges, 4 ticks | 563 | 32448 |
| join_300 | 300 x 300 rows, 75 keys, 4 ticks | 422 | 89072 |
| join_700 | 700 x 700 rows, 175 keys, 4 ticks | 2304 | 402384 |
| reduce_600 | 600 rows, 12 groups, 4 ticks | 34 | 3216 |
| reduce_2000 | 2000 rows, 12 groups, 4 ticks | 130 | 8240 |

Wide corpus: 203 emitted fixture plans, kernel arm at base grades 82 clean,
88 byte-diff, 33 runtime error. That corpus is the identity net, not a target;
the 121 non-clean plans must keep printing exactly the bytes they print now.

## Pre-existing failures found at base

| what | receipt | owner |
|---|---|---|
| `grade.sh` never reaches its own fixtures | its first line runs plunit, and `catalog_plane_rail:level_plane_family_corpus_counts` fails on `a65ca7e7` under `set -e` | `v6/prolog`, not this lane |
| fixture 3 errors out | `dd-runner: projection source missing: group`; the reduce group key is spelled `b0.group` in `bindings` and `group` in `aggregate.group`, and `eval_reduce` looks the bare name up in a row keyed by the aliased one | `kernel.rs:125` |

Fixture grading here runs `grade.sh`'s `grade()` body with the plunit line
removed, so the fixture verdicts are the same verdicts `grade.sh` would print.

## Dependency decision

Pending.

## Defect 3: linear membership

Pending.

## Defect 2: full cross product

Pending.

## Defect 1: clone and rederive

Pending.

## Gate output

Pending.

## Closing table

Pending.
