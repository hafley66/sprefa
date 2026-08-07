# P0-A: TEXT-key interning cost, measured

Lane P0-A of plans/2026-08-07-plan-ir-offload-contract.md phase 0. Commit
c93c5a40 on lane/p0a-intern-bench, base 86528155. All files under
intern_bench/; `git diff --stat 86528155 -- ../harness` is empty, so the three
banked `.in` cases cannot have moved.

## 1. The answer

| question | measured answer |
|---|---|
| does interning eat the walk's win | no; 0.06% of total at 7.7k input edges, 4.2% at 1M input edges |
| does the walk slow on TEXT-shaped data once interned | no; 97-104% of interp's same-session int rate, inside run noise |
| cost of keeping 4 raw TEXT columns instead of collapsing to node ids | 78-100% of interp; worst case grid_10000 at 78.3% |
| the real TEXT tax | materializing 10M derived rows back to TEXT: 230-251 ms, 40-43M rows/s, 8-15% of total |
| SQLite insert, TEXT vs interned INTEGER, same 4-column WITHOUT ROWID PK | INTEGER 1.7x-2.0x faster at every volume from 4k to 1M rows |

The offload's projected numbers hold for real TEXT-keyed modules. Interning is
cheap at the front; the money is at the back, in materialize plus the keyed
insert, which contract section 4.3 already suspected.

## 2. Full results, best of 3, one session

Darwin arm64, Apple M2 Pro, rustc 1.97.0-nightly, cargo release, single
threaded, no allocator feature. `load` = read + intern + seed the edge
relation (the same three things interp charges to load). `intern %` is against
load + fixpoint + materialize.

| case | engine | derived | fp ms | fp rows/sec | vs interp-int | load us | intern us | materialize ms | intern % |
|---|---|---|---|---|---|---|---|---|---|
| chain_10000 | interp-int | 9,996,213 | 1,429 | 6,995,250 | 100.0% | 1,000 | - | - | - |
| | pair | 9,996,213 | 1,380 | 7,244,908 | 103.6% | 2,023 | 942 | 248 | 0.058% |
| | pair-flat | 9,996,213 | 1,330 | 7,518,493 | 107.5% | 1,702 | 939 | 249 | 0.059% |
| | col4 | 9,996,213 | 1,429 | 6,995,911 | 100.0% | 1,743 | 757 | 251 | 0.045% |
| grid_10000 | interp-int | 1,069,200 | 144 | 7,425,000 | 100.0% | 0 | - | - | - |
| | pair | 1,069,200 | 149 | 7,199,272 | 97.0% | 828 | 452 | 25 | 0.259% |
| | pair-flat | 1,069,200 | 152 | 7,017,313 | 94.5% | 809 | 475 | 26 | 0.265% |
| | col4 | 1,069,200 | 184 | 5,815,990 | 78.3% | 746 | 378 | 26 | 0.180% |
| chain_1000000 | interp-int | 9,999,890 | 2,402 | 4,163,151 | 100.0% | 313,000 | - | - | - |
| | pair | 9,999,890 | 2,462 | 4,061,640 | 97.6% | 456,659 | 133,136 | 230 | 4.229% |
| | pair-flat | 9,999,890 | 2,481 | 4,031,147 | 96.8% | 337,046 | 131,880 | 228 | 4.330% |
| | col4 | 9,999,890 | 2,690 | 3,718,024 | 89.3% | 300,109 | 93,909 | 232 | 2.915% |

At 1M input edges the load comparison is real: 313 ms int vs 457 ms TEXT,
+144 ms (+46%), of which 133 ms is the intern pass and 23 ms is reading a
12.2x larger file. Intern throughput there: 999,989 edges in 133 ms = 7.5M
edges/sec (30M string lookups/sec plus 15M pair lookups/sec).

Same-session drift vs banked STANDINGS.md (why the gate demanded reruns):
chain_10000 -3.8%, grid_10000 -8.3%, chain_1000000 -12.4%.

## 3. SQLite: 4-column TEXT vs interned INTEGER

rusqlite 0.32.1 bundled, in-memory, page_size=16384 + temp_store=MEMORY (the
`chosen` set), one transaction, one prepared INSERT OR IGNORE, fresh DB per
run, best of 5. Both tables 4-column WITHOUT ROWID PKs with __refcount; only
the declared column type differs.

| source | rows | TEXT ms | TEXT rows/sec | INTEGER ms | INTEGER rows/sec | speedup |
|---|---|---|---|---|---|---|
| grid_10000 edges | 3,960 | 2.3 | 1,729,257 | 1.2 | 3,396,226 | 1.96x |
| chain_10000 edges | 7,743 | 4.1 | 1,903,392 | 2.3 | 3,310,389 | 1.74x |
| synth | 10,000 | 5.2 | 1,930,501 | 3.1 | 3,243,593 | 1.68x |
| synth | 100,000 | 61.5 | 1,627,100 | 31.9 | 3,137,451 | 1.93x |
| chain_1000000 edges | 999,989 | 672.3 | 1,487,412 | 341.8 | 2,925,484 | 1.97x |
| synth | 1,000,000 | 657.6 | 1,520,727 | 329.8 | 3,032,324 | 1.99x |

Both rates decay with table size (TEXT -21% from 10k to 1M rows), so a 10M
projection is a floor: TEXT >= 6.6 s, INTEGER >= 3.3 s. `stored` equals `rows`
in every run. The 10M derived insert itself is lane P0-B's; this lane stops at
1M so the two do not collide.

## 4. Materialize, the cost the brief did not scope

| case | derived rows | materialize ms | rows/sec | TEXT bytes | % of load+fp+mat |
|---|---|---|---|---|---|
| chain_10000 | 9,996,213 | 248 | 40,307,310 | 1,171,353,297 | 15.2% |
| grid_10000 | 1,069,200 | 25 | 42,768,000 | 124,131,240 | 14.4% |
| chain_1000000 | 9,999,890 | 230 | 43,477,782 | ~1.17 GB | 7.9% |

Materialize costs 1.5x-1.7x the intern pass at the 1M-edge input and 10x it at
the 10k points, and is small next to the insert it feeds: 230 ms against
>= 6,600 ms of TEXT insert for the same 10M rows, a 1:29 ratio.

## 5. Memory (finding for P1-C)

| case | interp-int | pair | pair-flat | col4 |
|---|---|---|---|---|
| chain_10000 | 1,380,320 kb | 1,391,648 kb | 1,180,432 kb | 1,179,456 kb |
| grid_10000 | 186,112 kb | 207,488 kb | 219,536 kb | 227,952 kb |
| chain_1000000 | 2,132,400 kb | 2,117,008 kb | 1,193,520 kb | 1,735,520 kb |

interp's membership set shards on column 0, so the shard vector is as long as
the node count: at 1,052,620 nodes that costs 924 MB over a flat set for a
0.8% rate difference. sprefa-fixpoint should take the flat set.

## 6. Generator design call

A separate crate that path-depends on the harness library, zero lines changed
in harness/. gen_text calls tuner::tune + gen::generate with the harness's own
seed formula, so topology equality is by construction; `--also-int` re-renders
the same edge list in the harness .in format and run.sh gates on `cmp`. The
int twin matched cmp-exact for chain_10000, grid_10000, layered_10000 and
chain_1000000.

## 7. The TEXT key shape

Mirrors gen_emitted/flagship_flow_reach_over_batched_resolved_edges.ts:137
DDL. Node N -> path `src/engine/lower/pass_{N/50/40}/module_{N/50}.ts`, name
`resolveBindingStep_{N%50}`: 50 symbols per file, 40 files per directory, path
38-40 bytes with a 24-byte shared prefix (asserted by a unit test).
chain_10000.tin is 915,083 bytes vs chain_10000.in's 75,225.

Walk modes: pair (tuple, engine2.rs copied verbatim from interp), pair-flat
(flat membership set, the control), col4 (raw 4 string ids, flat set,
2-column index packed to u64). col4 is not a clean isolate (no single column
to shard on); pair-flat is the control, col4 vs pair-flat is the arity number
(chain 1M: -7.8%).

## 8. Correctness

Every mode on every case agrees with interp on derived AND checksum, same
session (order-independent XOR of fnv1a64 over the recovered int node ids):
chain_10000 df09b2f409f8b9a8, grid_10000 9d7239568960d6a8, chain_1000000
39f95731ca7b3154. 8 unit tests pass, including pair_checksum pinned against
interp's hand-computed value.

## 9. Reproducing

```
cd v6/labs/exec_shootout/intern_bench
bash run.sh                                                 # chain + grid @10000
FAMILIES=chain SCALE=1000000 SYNTH=1000000 bash run.sh /tmp/big
cargo test --release                                        # 8 passing
```

run.sh builds release, regenerates both input forms, fails loudly if the int
twin is not byte-identical, runs RUNS=3 reps of interp + three modes, runs the
SQLite race, prints best-of via best.awk. Raw per-run JSONL at
<work>/results.jsonl.

## 10. Notes

rusqlite is a new dep confined to this bench dir (the engines' CONTRACT.md dep
rules untouched). The first load-phase attempt left the edge seed outside both
clocks and was discarded after the fix; reported numbers all post-fix.
