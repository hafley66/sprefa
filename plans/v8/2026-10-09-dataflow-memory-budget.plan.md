# Main dataflow path: memory budget per phase

User 2026-10-09: "correctly structuring program runtime memory alloc sequencing by main dataflow
path budget analysis"; "term table/interning is fine just dont make a bespoke piece of shit";
"have the universe relationally warm across frontiers and figure out dd storage/batching to make
sqlite syncing efficient".

## Ledger (grapht, 148,043 TSI rows, `--features alloc-ledger`)

`RUST_LOG=dl8::memory=info dl8 compile fixtures/extract/main.dl7 --project fixtures/extract --tsi <grapht>`

| phase | ms | live MB | peak MB |
|---|---|---|---|
| read | 310 | 31 | 58 |
| macrotime | 138 | 63 | 64 |
| lower | 1210 | 82 | 107 |
| check | 37 | 86 | 86 |
| comptime | 2760 | 193 | 334 |
| compile done | | 180 | 219 |
| program + write, before | | 2509 | 3717 |
| program + write, after | | 93 | 180 |

Process footprint: 8 GB kill (main before `43010e26c`) → 2.37 GB → 0.24 GB.
Output bytes unchanged: 150 fixtures `cmp`, grapht sha256 `9e212a1e…`.

## Output size

12,625,025,051 bytes. The owner `module(tsi("tsc", [488 digests]))` is about 39 KB of text
and prints 321,146 times. The Universe holds it once (`term_index`, `_0_term.rs:242`); the
JSON boundary reprints the tree per use.

## Top 5

| # | idea | state | blocks on |
|---|---|---|---|
| 1 | stream compile output; phase ledger on the counting allocator in hafley-observe | done, branch `perf/phase-memory-ledger` | merge waits on the dirty `src/lib.rs` in `sqlite-perf-main` |
| 2 | term relation as output: `term(id, functor, args)` rows from the DD term map, rows name ids | design | id collapse (TermId = Cell); output format change refreezes goldens |
| 3 | Universe warm across frontiers: the term relation is a DD input, mint is +1, sweep is −1 (`universe-as-ivm.design.md`) | design | term-lifetime S3 (dirty `_9a`, `_9b`, `_9d`, `_9a1` in `sqlite-perf-main`) |
| 4 | sqlite sync from consolidated DD batches per frontier, columnar layout | lab measured: `labs/20261008.0…` col-row-sqlite 1,503 MB vs vec-row 2,250 MB at 8M keys | 3 |
| 5 | budget gate: memcap in dl8 and a ladder test (lad1 … lad64) that fails when a phase's peak grows faster than its rows | not built | hafley-observe dep checkout at `a3002df0` for memcap |

## Round trips seen

- `_5_reify/_4_emit.rs:114`, `src/bin/dl8.rs` eval: term → JSON `Value` → `Program` in one process.
- `_6_eval/_9c_ir_bundle.rs:114`: one `term_to_json` `Value` per constructor term (`printed`).
