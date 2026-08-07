# REPORT-TAIL: phase-1 tail alone, measured on real data with no walk

What this measures: given the true 10M-row closure already derived and sitting in
`reachable` in derivation order, how long does the SQLite "tail" cost to land it
in the head `r`? The walk is excluded by construction (the closure comes from the
`loop_range_rowid` winner, so the only timed statements are the tail fills and
copies, all after `WINNER.derive` returns). This is the phase-1 ceiling measured
rather than projected (plan section 4.3 / 5, P0-B).

Scope correction honored: tail A is the unobserved-rel skip shape (fill `__new_r`
+ `INSERT OR IGNORE` head, no staging copies); tail B adds the `__delta_r` and
`__frontier_r` copies the unobserved-rel skip deletes. The skip's value is the gap
between them.

## Method, in one line

`node exp_tail.mjs` -> `exp_tail.mjs`: open `:memory:` libsql (`chosen` pragmas:
`page_size=16384`, `temp_store=MEMORY`, matching the sqlite_raw convention),
create the winner's `reachable` (bare rowid + unique index), load edges, run
`variants.loop_range_rowid.derive` once (not timed), then create the tsv2 tail
tables (`__new_r` bare-rowid, head `r` WITHOUT ROWID with `__refcount` default 1,
and for tail B `__delta_r` / `__frontier_r`) all `CREATE TEMP TABLE`, and time the
tail statements. Every table and copy statement mirrors the emitted engine's
shape. Best of 2 for `chain_10000`, single run for the other two. Inputs
regenerated deterministically by the shootout harness
(`harness --engines ref --scales 10000`) because the banked `.in` files were
missing from the tree; each regenerated case re-verifies its banked derived count
before being timed (grid 1,069,200; chain 9,996,213; layered 9,951,396).

## Results

| case | tail A fill | tail A insert | tail A total | antijoin variant | tail B total | head |
|---|---|---|---|---|---|---|
| grid_10000 | 86 | 352 | **438** | 472 | 661 | MATCH |
| chain_10000 | 809 | 6,789 | **7,598** | 7,863 | 9,711 | MATCH |
| layered_10000 | 817 | 3,382 | **4,200** | 4,555 | 6,296 | MATCH |

Every ms is from a run I executed. ms are rounded to whole milliseconds; the tail
A / antijoin columns are best-of-2 on chain, single on grid and layered. Each
run's head row count equals the banked derived count, so the head-verification
cell is MATCH on every case. Tail A total = fill + insert summed; the antijoin
variant is `INSERT INTO __new_r ... REACHABLE n LEFT JOIN r h WHERE h.source IS
NULL` (the emitted FillNewSql shape) + the same insert; tail B total = tail A +
delta copy + frontier copy.

Antijoin shape source: no emitted fixture carries a recursive
(`expandSql`/`dredSql` non-null) statement, so I mirrored the FillNewSql antijoin
text from the flat emitted head `pair` in
`v6/tsv2/gen_emitted/combine_level_is_the_conjunction_spelling.ts`:
`INSERT INTO "__new_" SELECT n.* FROM <its single staging table> n LEFT JOIN
"pair" h ON n."left" = h."left" AND n."right" = h."right" WHERE h."left" IS
NULL` (the fixture's single staging table carries the engine's `__new_`-adjacent
prefix in the emitted text, omitted here), renamed to `reachable` (staging) and
`r` (head) on the two key columns source, target. The
antijoin form is identical between flat and recursive heads (both `FillNewSql`,
the recursive path reads its staging as one wave table); the head is empty at the
cold-build fill, so this mirrors the phase-1 cold-build gate, not a later tick.

## Verdict

**phase-1 tail ceiling on chain_10000 = 7,598 ms vs the 12,000 ms gate.** The
tail alone is 37% under the gate, so phase 1 as specified is not dead before any
IR is written. The projected 9,100 ms total fixpoint (section 4.3) is within
reach: 7,598 ms tail + ~1,400 ms executor leaves room under the 12,000 ms bar.

## Which statement dominates

The `INSERT OR IGNORE INTO r` (source, target) into the WITHOUT ROWID head is the
whole cost: 6,789 ms on chain, 89% of tail A's 7,598 ms and 2.1x the layered
number on the same row count. The fill (`INSERT INTO __new_r ... ORDER BY rowid`)
is 809 ms on chain, two columns into a bare rowid append table, and the antijoin
LEFT JOIN adds only ~265 ms over that fill (7,863 vs 7,598) because the cold-build
head is empty. On grid the insert is 352 ms of a 438 ms total (80%) and on layered
3,382 ms of 4,200 (81%). The pattern is uniform: the bare-rowid `__new_r` fill is
cheap, the WITHOUT ROWID keyed head insert is the floor, exactly as the plan's
"the ceiling is the insert, not the walk" reading predicted. The unobserved-rel
skip is worth 9,711 - 7,598 = 2,113 ms on chain (~26% of tail B); the delta copy
and frontier copy are the next two costs after the head insert.

## Reproducing

```
cd v6/labs/exec_shootout/harness && cargo build --release
./target/release/harness --engines ref --scales 10000 --work target/release/work \
  --standings ../STANDINGS.md        # regenerates grid/chain/layered_10000.in
mkdir -p ../dl6/.bench && cp -f target/release/work/*_10000.in ../dl6/.bench/
cd ../sqlite_raw && node exp_tail.mjs # all three cases
node exp_tail.mjs --only <case>       # one case
```

`exp_tail.mjs` reuses `common.mjs` (`openDatabase`, `readEdges`, `loadEdges`) and
`variants.mjs` (`loop_range_rowid.derive`). No existing file was edited.
