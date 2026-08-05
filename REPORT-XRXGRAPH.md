# REPORT-XRXGRAPH — exec_shootout rx operator graph lane

## The lane

`v6/labs/exec_shootout/rxgraph/` is a standalone cargo crate (workspace-excluded from the
repo root). It implements the second of the three shootout execution strategies: the
program is a graph of boxed operator trait objects (map, filter, distinct, join) wired at
startup, and deltas flow between them through trait-object calls. The dynamic-dispatch
indirection is the layer this lane measures against the interp and mono neighbors.

## What was built

| path | role |
|---|---|
| `Cargo.toml` | deps `fxhash`, `libc` only; empty `[workspace]` to eject from the root workspace |
| `src/lib.rs` | `Operator` trait + `MapOp` / `FilterOp` / `DistinctOp` / `JoinOp` / `SinkOp` boxes; `Program` graph + semi-naive driver; `build_reachability` wiring; fnv1a64 checksum |
| `src/main.rs` | CLI (`--input`), input parser, JSONL events, `getrusage` peak RSS |
| `tests/cli.rs` | integration test: binary emits exactly three parseable JSONL events |

The reach program lowers to the contract's rx shape exactly: `seed_distinct` (Distinct)
fans out to the join's left input and to a sink; the join carries the static edge index on
its right side; `fixpoint_distinct` (Distinct, sharing one global seen set with the seed)
dedupes join output and feeds it back to the join. The queue drains when an empty batch is
dropped at its source, which is the required empty-batch stop.

Wiring:

```text
edge seed -> seed_distinct -> join (left) and sink
                           join -> fixpoint_distinct -> join (feedback) and sink
edge index statically owned by join (right side)
sink accumulates derived count and checksum-fold
```

## Ground rules held

- No rayon; single thread, no `--threads` flag needed (reserved per contract, default 1).
- Comments state only constraints the code cannot show and stay within two consecutive
  lines. No em dashes anywhere. Banned words (provenance, substrate, load-bearing, regime)
  absent from prose and identifiers.
- Descriptive identifiers throughout, no single-letter names (loop indices included).
- Exit 0 on success; nonzero plus one stderr line on any failure.

## Gates (ran in the crate dir)

```
cargo build --release   ->  Finished release profile [optimized] in ...
cargo test --release    ->  4 lib tests + 1 integration test, all ok
```

Unit tests required by the brief:

| test | asserts |
|---|---|
| `semi_naive_stops_on_empty_delta` | 4-node chain: derived=6, max_round=2, operator_pushes=9 (terminates) |
| `checksum_matches_hand_computed_value` | 3-edge chain: checksum == `0e0086019623ec40`, derived=6 |
| `cli_emits_three_jsonl_events` | loaded/fixpoint/done events parse as JSON with the required fields |

## Hand-run (10 edges, expected derived computed by hand)

Input: an 11-node chain `0 -> 1 -> ... -> 10`, 10 edges. Its reachable closure is every
ordered pair `(i, j)` with `i < j`, count C(11, 2) = 55.

```
p 11 10
0 1
1 2
2 3
3 4
4 5
5 6
6 7
7 8
8 9
9 10
```

```
{"event":"loaded","edges":10,"ms":0}
{"event":"fixpoint","derived":55,"ms":0}
{"event":"done","checksum":"298615f12c00bb95","peak_rss_kb":1440}
```

`derived` = 55, matching the hand count. A second hand-run on the 3-edge chain confirmed
the binary checksum `0e0086019623ec40` equals the value computed by hand in the unit test.

## Deviations

None.
