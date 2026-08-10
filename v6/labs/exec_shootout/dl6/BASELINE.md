# sqlite build fixpoint baseline (dl6 bench)

THE number the prolog emitter ratchets against: best pure SQLite can do on the
three dl6 cases with nothing in the way. No node, no prolog, no runtime, no dd.
One crate `v6/labs/exec_shootout/sqlite_baseline/`, pure rust + rusqlite
(bundled SQLite), in-memory database. Semantics: transitive closure
`reachable(s,t)` from `edge(s,t)`, set semantics. Every banked run matches the
dl6 agreement (derived count + checksum) exactly; a mismatched run is INVALID
and was never banked.

Machine: Darwin 24.6.1 (arm64) · rustc 1.97.0-nightly (9eb3be26b 2026-05-18) ·
cargo 1.97.0-nightly · date 2026-08-10T18:35:51Z

## Reproducing

```bash
cd v6/labs/exec_shootout/sqlite_baseline
cargo build --release
B=./target/release/sqlite_baseline
$B --case grid_10000    --variant naive       --runs 3
$B --case layered_10000 --variant tuned_range --runs 3
$B --case chain_10000   --variant tuned_wave  --runs 3
# --list-cases / --variant naive|tuned_wave|tuned_range
```

The generators are ported 1:1 from `harness/src/gen.rs` (grid rows=45 cols=45,
layered layers=193 width=26 fanout=2 seed=scale^0x5eed_cafe, chain
segment_len=2582), not read from another tree's `.bench` files; the edge counts
are asserted in code and match the table below exactly. Checksum is the exact
fnv1a64 XOR fold from `dl6/bench.ts` (fnv1a64 over the 8 LE bytes of
source,target, XOR-folded, order-independent).

## Contract numbers, best of 3 per cell

`fixpoint ms` = derivation phase wall only (load and checksum fold excluded);
best of 3. `peak RSS` = single-run process maximum resident set (getrusage,
cross-checked against `/usr/bin/time -l`, identical).

| case | variant | best fp ms | peak RSS (MB) | derived | checksum | rounds | statements |
|---|---|---|---|---|---|---|---|
| grid_10000 | naive | 1462 | 31.1 | 1,069,200 | `9d7239568960d6a8` | 1 | 1 |
| grid_10000 | tuned_wave | **1065** | **16.2** | 1,069,200 | `9d7239568960d6a8` | 87 | 265 |
| grid_10000 | tuned_range | 1080 | 37.1 | 1,069,200 | `9d7239568960d6a8` | 87 | 89 |
| layered_10000 | naive | 14510 | 246.2 | 9,951,396 | `addcf85b5162b9da` | 1 | 1 |
| layered_10000 | tuned_wave | 10948 | 114.0 | 9,951,396 | `addcf85b5162b9da` | 191 | 577 |
| layered_10000 | tuned_range | **10254** | **302.5** | 9,951,396 | `addcf85b5162b9da` | 191 | 193 |
| chain_10000 | naive | 19674 | 248.5 | 9,996,213 | `df09b2f409f8b9a8` | 1 | 1 |
| chain_10000 | tuned_wave | 12210 | 114.1 | 9,996,213 | `df09b2f409f8b9a8` | 2580 | 7744 |
| chain_10000 | tuned_range | **11224** | **297.8** | 9,996,213 | `df09b2f409f8b9a8` | 2580 | 2582 |

Best per case is bold. Run-to-run fixpoint variance is ~20% (machine load);
a single run of chain tuned_range landed 9717 ms, its best-of-3 is 11224 ms.

## Banked best per case

| case | variant | fixpoint ms | peak RSS MB | vs dl6 fixpoint | vs dl6 peak RSS |
|---|---|---|---|---|---|
| grid_10000 | tuned_wave | 1065 | 16 | 1265 -> 1065 (1.19x) | 715 MB -> 16 (45x) |
| layered_10000 | tuned_range | 10254 | 303 | 11721 -> 10254 (1.14x) | 1266 MB -> 303 (4.2x) |
| chain_10000 | tuned_range | 11224 | 298 | 20850 -> 11224 (1.86x) | 1467 MB -> 298 (4.9x) |

Every banked cell beats dl6 on both wall and peak RSS. The pure-SQLite
build-fixpoint floor is ~10.5% (layered) to ~1.9x (chain) faster than dl6 on
time and 4 to 45x leaner on memory for the non-naive engines.

## Variants

### naive
One statement: `INSERT INTO reachable ... WITH RECURSIVE closure ... UNION`
into a WITHOUT ROWID `(source,target)` result. This is the competent-first pass:
a recursive CTE, no round loop. Fast on small shapes, worst on the deep chains
because the single recursive statement carries the whole closure at once.

### tuned_wave
Semi-naive ping/pong wavefront. `reachable` and both `frontier_ping` /
`frontier_pong` are WITHOUT ROWID integer-key tables. Each round: clear the idle
frontier, hop `frontier JOIN edge` gated by a NOT EXISTS against `reachable`
(dedupe is the in-tree PK of the frontier), promote survivors into `reachable`
via `INSERT OR IGNORE`. Citation: sql-relational-design (surrogate integer keys,
WITHOUT ROWID, no fat keys), sqlite-costs (WITHOUT ROWID beats rowid+unique on a
fixpoint head, 2.4x leaner: 35.5 vs 15.0 MB grid; the ping/pong pair avoids a
truncate-between-read-and-write and skips a second index probe). Wins grid, is
the leanest RSS on every case (17-120 MB).

### tuned_range
Semi-naive hop where the delta is a rowid range, not a table. `reachable` is a
rowid table with `UNIQUE(source,target)`; each round inserts the survivors of
`reachable(rowid BETWEEN low,high) JOIN edge` via `INSERT OR IGNORE`, and the
round's rowid span is advanced by the previous round's `changes()`. No frontier
table, no second btree write per row, `changes()` drives the loop and the next
range, one transaction. Citation: sqlite-costs (the rowid-range delta is worth
17-53% and requires a rowid; OR IGNORE rejection beats a NOT EXISTS prefilter).
Wins layered and chain, tied on grid. This is the variant whose open-loop form
was 3.4x slower until the single transaction was added (per-statement commit on
2580 chain rounds), matching the cited statement-dispatch/vs-transaction cost.

Peak RSS per variant (chain_10000): tuned_wave 120 MB, naive 273 MB,
tuned_range 305 MB. WITHOUT ROWID pays the leanest memory as the
sql-relational-design law predicts.
