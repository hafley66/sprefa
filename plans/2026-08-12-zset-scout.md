# dd, timely, and dbsp scout

## TOC

1. [Relation](#relation)
2. [Principal upstream cases](#principal-upstream-cases)
3. [The gap](#the-gap)
4. [Implementation and measurements](#implementation-and-measurements)
5. [Unimplemented work](#unimplemented-work)
6. [DBSP unblock](#dbsp-unblock)

## Relation

| Project | Core object | Time and execution | Relationship and source |
|---|---|---|---|
| Timely Dataflow | Timestamped streams and operators | Partially ordered progress frontiers, worker scheduling, scopes | Runtime used by Differential Dataflow. The timely README describes workers, operators, and progress tracking: [timely README](https://github.com/TimelyDataflow/timely-dataflow/blob/master/README.md). |
| Differential Dataflow | Collections of `(data, time, diff)` and indexed arrangements | Partially ordered timestamps, iterative scopes, arrangement-backed joins, signed differences | Built on timely and adds incremental relational operators. [DD README](https://github.com/TimelyDataflow/differential-dataflow/blob/master/README.md), [reachability example](https://github.com/TimelyDataflow/differential-dataflow/blob/master/examples/reachability.rs). |
| DBSP crate | Z-sets and circuits with differentiate/integrate operators | Nested circuit clocks and circuit execution | Shares integer-weighted relation algebra with DD. The circuit model and clock nesting are documented in [DBSP](https://github.com/feldera/feldera/tree/main/crates/dbsp). |
| This repo | Reachability over generated edge relations | Shootout engines use a semi-naive fixpoint; store engines include DD and DBSP adapters | Existing families are chain, layered, and grid. The harness contract defines the output checksum and derived-row truth: [`CONTRACT.md`](../v6/labs/exec_shootout/CONTRACT.md). |

## Principal upstream cases

| Project | Correctness cases in shipped code | Benchmarks or examples in shipped code | Requested dimension |
|---|---|---|---|
| Differential Dataflow | `tests/`, `tests/replay.rs`, `tests/arrange.rs`, `tests/join.rs`, `tests/iterate.rs`; `examples/reachability.rs` | `benches/`, arrangement and spine benchmarks, reachability example | reachability, joins, iteration, signed updates and retractions |
| Timely Dataflow | `timely/src/progress/`, `timely/src/dataflow/`, `timely/tests/`; operator and progress tests | `timely/benches/`, `timely/examples/`; communication and operator throughput | progress/frontier behavior, nested scopes, worker coordination, latency |
| DBSP / Feldera | `crates/dbsp/src/circuit/`, `crates/dbsp/src/operator/`, `crates/dbsp/src/trace/`, and crate tests | `crates/dbsp/benches/`, SQL integration tests and Feldera benchmark suites | Z-set updates, circuit feedback, joins, aggregation, arrangements, incremental updates |

The upstream repositories evolve independently. The paths above are repository
paths and should be checked against the exact dependency revisions before using
them as a versioned citation. The DD reachability implementation is directly
available in the linked example. DD's input API explicitly accepts positive and
negative differences and arbitrary integer weights in [Making changes](https://timelydataflow.github.io/differential-dataflow/chapter_3/chapter_3_3.html).

## The gap

| Case | Existing coverage | Gap status |
|---|---|---|
| Reachability | All three existing families run the same reachability rule | Present as the semantic workload, absent as a named `reach` family |
| Grid | `grid` family and emitted grid bench | Present |
| Long chain | `chain` family | Present |
| Layered joins | `layered` family | Present |
| Cycles and SCC-shaped closure | Store rig and DRed fixtures cover cycles; shootout did not have a cyclic family | Added as `cycle` |
| Negative weights and retraction latency | DD/store and dl6 retraction benches | Missing from the rust shootout protocol, whose input contract is static positive edges |
| Aggregation | No corresponding shootout family | Missing |
| Timely frontier and worker latency | No corresponding shootout metric | Missing |
| DBSP circuit-clock latency | DBSP adapter exists but head-to-head is blocked | Missing |

## Implementation and measurements

Added `Family::Cycle` to the existing shootout generator and tuner. It creates
disjoint directed cycles, preserving the existing reachability input contract,
engine binaries, checksum, row-count validation, and best-of-three protocol.
The tuned scales target approximately five million derived rows:

| Scale | Components | Component size | Expected derived rows |
|---:|---:|---:|---:|
| 10,000 | 100 | 223 | 4,972,900 |
| 100,000 | 1,000 | 70 | 4,900,000 |
| 1,000,000 | 10,000 | 22 | 4,840,000 |

`cargo test --manifest-path v6/labs/exec_shootout/harness/Cargo.toml` passed 10
tests after the change. A complete shootout gate transcript is in `REPORT.md`.

## Unimplemented work

The static edge-file protocol in [`CONTRACT.md`](../v6/labs/exec_shootout/CONTRACT.md)
has no input-update, timestamp, signed-weight, aggregation, or latency event.
Those cases require protocol changes outside the owned harness case definitions.
The existing grid case was verified by the pre-existing generator, tuner, and
banked `grid_10000` checksum in `v6/justfile`.

## DBSP unblock

The DBSP head-to-head was left unchanged. The brief records the current block:
the `with-dbsp` feature must be built on stable with the linker flag, while the
default nightly build path ICEs and `cargo build --examples` omits the feature.
No stable feature build was landed in this lane.
