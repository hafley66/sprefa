# REPORT-XINTERP — interp lane

The IR-interpreter engine for the exec_shootout reachability workload. One
Rust crate at `v6/labs/exec_shootout/interp/` producing one binary `interp`.

## Changes

- `interp/Cargo.toml`: standalone crate (empty `[workspace]` keeps it out of
  the repo-root workspace). Edition 2021. Deps: `rustc-hash 2.1.3`, `libc
  0.2.189` (both on the contract allowlist; nothing else).
- `interp/src/main.rs`: the full engine. The workload's two rules are
  expressed as IR DATA (`Rule`/`Atom`/`Term` structs), relations live in a
  generic tuple store (`rows` + `members` dedup set + per-column `index`),
  and `semi_naive` re-reads `program.rules` each batch. Nothing names
  reachability.
  - `match_body` is a right-deep join: the delta atom scans its listed rows,
    later atoms probe the column index on bound join variables.
  - `parse_input` reads `p <nodes> <edges>` then `u v` lines; duplicate
    edges are deduped and the distinct count is reported as `edges`.
  - Emits exactly the three JSONL events; `checksum` = XOR of
    `fnv1a64(u_le ++ v_le)` over all derived pairs; `peak_rss_kb` from
    `libc::getrusage` (`ru_maxrss / 1024` on macOS).
- Unit tests (3): semi-naive stops on an empty delta; checksum matches a
  hand-computed value on a 3-edge graph; loaded/fixpoint/done strings
  re-parse into their expected fields.

## Gates (verbatim)

Run in `v6/labs/exec_shootout/interp/`.

```
$ cargo build --release
   Compiling interp ... Finished `release` profile [optimized] target(s) in 0.41s

$ cargo test --release
   running 3 tests
   test tests::checksum_matches_hand_computed_three_edge ... ok
   test tests::events_parse_as_json ... ok
   test tests::semi_naive_stops_on_empty_delta ... ok
   test result: ok. 3 passed; 0 failed
```

Hand-run against a tiny 10-edge input (three disconnected chains, 3 + 3 + 4
edges):

```
p 13 10
1 2
2 3
3 4
5 6
6 7
7 8
9 10
10 11
11 12
12 13
```

Expected derived count, computed by hand: chain on nodes 1-4 gives C(4,2)=6
pairs, chain on nodes 5-8 gives 6, chain on nodes 9-13 gives C(5,2)=10.
Total = 22.

```
$ ./target/release/interp --input /tmp/tiny10.txt
{"event":"loaded","edges":10,"ms":0}
{"event":"fixpoint","derived":22,"ms":0}
{"event":"done","checksum":"f216127d4e9ff8c8","peak_rss_kb":1600}
```

`derived=22` matches the hand count; `checksum` independently recomputed in
Python and matched.

Smoke at scale (join-heavy layered graph, for the harness band): a 10k-edge
case yielded 42.3M derived rows in ~33s (~1.3M rows/s).

## Deviations

None.
