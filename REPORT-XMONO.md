# REPORT-XMONO

## TOC
- What this lane is
- Location and ownership
- Changes
- Verbatim validation gates
- Hand-run on a tiny input (with hand-derived expectation)
- Deviations

## What this lane is

The `mono/` crate is the monomorphized-rust strategy of the exec_shootout
three-way measurement. The program is two-rule semi-naive reachability; the
source is written as a souffle-style generator would emit it: concrete `u32`
pairs, a concrete `FxHashMap` y-join index, and the unrolled rule loop, in one
file (`src/main.rs`).

## Location and ownership

- Crate: `v6/labs/exec_shootout/mono/` (only this dir is owned).
- `src/main.rs` is the exhibit (258 lines plus inline `#[cfg(test)]` tests in
  the same file, so the visible output shape has zero indirection).
- Standalone crate: an empty `[workspace]` table in `Cargo.toml` keeps it from
  joining the repo-root `sprefa-dl` workspace, matching the other shootout
  lanes.
- Deps: `fxhash` (y-join index + derived set), `libc` (getrusage). Within the
  contract allowlist.

## Changes

| file | what |
|---|---|
| `Cargo.toml` | package `mono`, edition 2021, `fxhash` + `libc`, empty `[workspace]` |
| `Cargo.lock` | generated for the standalone crate |
| `src/main.rs` | full IO contract: `--input` parse, 3 JSONL events to stdout, fnv1a64+ xor checksum, peak RSS via `ru_maxrss/1024`; the y-join index, the unrolled loop; 3 unit tests |

The semi-naive loop is the exhibit: rule 1 only seeds (`derived` starts as the
edge set); the loop is a single delta-join of `reachable` on `y` against the
edge index, inserting only newly derived pairs, feeding back until the batch is
empty. The empty-batch condition is the stop.

## Verbatim validation gates

```
$ cargo build --release
    Finished `release` profile [optimized] target(s) in 0.01s

$ cargo test --release

running 3 tests
test tests::events_parse_as_json ... ok
test tests::checksum_matches_hand_computed ... ok
test tests::semi_naive_stops_on_empty_delta ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

The three unit tests required by the brief are present:
1. `semi_naive_stops_on_empty_delta` — the single-edge graph `1 2` seeds
   `reachable(1,2)`, the first delta-join finds no next hop, the loop stops
   with an empty delta.
2. `checksum_matches_hand_computed` — on the 3-edge graph `1 2 / 2 3 / 1 3`
   the xor of three hand-computed `fnv1a64` values equals the run's checksum.
3. `events_parse_as_json` — parses the three emitted JSONL lines with an
   inline parser (no json dep, per the allowlist).

## Hand-run on a tiny input (with hand-derived expectation)

Input (`/tmp/mono_tiny.txt`), 10 edges, 6 nodes:

```
p 6 10
1 2
1 3
2 4
2 5
2 6
3 4
3 5
3 6
4 5
5 6
```

Hand-derived transitive closure:

| source | reachable set | count |
|---|---|---|
| 1 | {2,3,4,5,6} | 5 |
| 2 | {4,5,6} | 3 |
| 3 | {4,5,6} | 3 |
| 4 | {5,6} | 2 |
| 5 | {6} | 1 |
| 6 | {} | 0 |

Expected `derived` = 5 + 3 + 3 + 2 + 1 = 14.

Run:

```
$ target/release/mono --input /tmp/mono_tiny.txt
{"event":"loaded","edges":10,"ms":0}
{"event":"fixpoint","derived":14,"ms":0}
{"event":"done","checksum":"451a513ce5081616","peak_rss_kb":1504}
```

`derived`=14 matches the by-hand count; `checksum`=`451a513ce5081616` matches
an independent brute-force closure/fnv1a64 computation. Note implemented as a
correctness check: the packed `(u<<32)|v` storage must hash
`u.to_le_bytes() ++ v.to_le_bytes()` (u first), not the packed u64 bytes.

## Deviations

None.
