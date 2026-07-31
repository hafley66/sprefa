# Rust lowering course (user 2026-07-31: "plot course for lowering the
# compiler into rust after we have language agnostic cli based benches")

Order is fixed by the directive: benches FIRST, rust after. The prolog
oracle stays the spec at every phase (oracle-as-spec is the defended
property from the design review). Nothing here starts until the beta wave
lands.

## Phase 0 — language-agnostic CLI bench contract (the gate)

One contract, any implementation language:

- An ENGINE is one executable: `engine run --program <file.dl6|module>
  --schedule <schedule.json> --db <path>` -> tick-log JSONL on stdout
  (the item-9 cross-target log format, canonical json text per the
  json_ticklog ruling), perf summary JSON on fd 3 or a `--perf-out` file
  (wall ms, ticks, statements, peak RSS, db bytes).
- CORRECTNESS leg: tick log byte-diffed against the oracle's log for the
  same program+schedule. An engine that cannot produce the log is not an
  engine (v1's asymmetry from the scale bench never happens again).
- TIMING leg: build-vs-buy FIRST per standing law. Candidates to research:
  hyperfine (CLI benchmark harness, warmups/statistics built in),
  bencher, BENCHER_DEV, plain /usr/bin/time -l loops. Expected verdict is
  hyperfine + our own correctness referee, but the lane writes the
  analysis before any bespoke line.
- Adapters at contract birth: tsv2 (`bop run` shape already close), the
  swipl oracle (dl6_oracle door), v5 rust where the program is expressible
  both sides (flagship rig precedent). Standings CSV extends
  PERF-REPORT.md conventions (same input hashes, memory columns,
  N/A-with-reason).

Exit receipt: one `just bench-cli` run producing a standings table over
the existing scale fixtures (s1/s2/s3, DAG/CYC) + at least one real
program (callgraph flagship), all engines byte-identical to the oracle.

## Phase 1 — rust target: strategy decision (needs the phase-0 numbers)

Two candidate shapes, DECIDED BY BENCH DATA not taste:

- (a) rust RUNTIME, emitted SQL unchanged: port the tsv2 runtime loop
  (tick loop, frontier tables, IVM statements) to rust over rusqlite;
  the prolog compiler keeps emitting the same statement plans, packaged
  as data (JSON) instead of TS modules. Smallest correct: the compiler
  front stays prolog, only the executor moves. The v5 store already
  proves the rust+sqlite IVM shape (count-IVM beat DRed 4-5x receipt).
- (b) rust EMIT target: emit_rs.pl beside emit_ts.pl, whole generated
  programs in rust. More surface, only worth it if (a)'s data plane
  shows JS-side costs that statement plans cannot escape.

Phase-0 standings decide: if tsv2's gap to v5-rust on the same statement
plans is driver/runtime overhead (the scale-bench A-runtime finding
pattern), (a) captures it. Measure, then choose.

## Phase 2 — compiler-front rust question (LAST, maybe never)

"Lowering the compiler into rust" ends here: the prolog front
(parse/analyze/expand/lower) compiles programs offline; its speed is
already gated by compile-speed and is not the felt gap (ingest/runtime
are). The front moves to rust only if a phase-1 receipt shows compile
time on real corpora matters. Until then prolog stays the one canonical
parser + oracle (design-review defended property D).

## Sequencing vs the beta wave

beta lanes (errors, float/avg, fork_join, docs) > phase 0 bench contract
> phase 1 strategy decision. Phase 0 is dispatchable now as its own lane
(new bench/ files only, disjoint from every running lane).
