# SCIP-SPEED.REPORT.md the scip-informed leg under the 10-second law (2026-08-30)

Lane: `fix-extract-scip-speed-3`. Base: 292888ba5 (ff from origin/main) merged
with origin/fix/extract-scip-speed, plus the dead-lane scip.rs patch applied as
017e23d3c. Binary: `v6/sprefa-extract/target/release/extract`.

## Contents

1. What the inherited commits and the recovered patch already cover
2. Task A: index the index (walls, before/after jsonl identity)
3. Task B: go scip rows against the vta bare oracle
4. Task C: informed-by-default when the index is fresh

## 1. What the inherited commits and the recovered patch already cover

| commit | what it is | Task A | Task B | Task C |
|---|---|---|---|---|
| 4084f4abb | the family stream emits the raw `scip_relationship` table (override flags ride the wire) | no | no | no |
| 32c016572 | one-process measurement, gap classification, SCIP.REPORT.md | no (it measured the slow seam, 113/163/436 s) | no | no |
| 6e1632824 | test file number + report title | no | no | no |
| 017e23d3c | scip.rs patch recovered from the killed lane: per-document sorted occurrence vectors, symbol->def map, address+fingerprint-keyed caches | **yes (the implementation)** | no | no |

The inherited commits are measurement and wire work only. The recovered patch
implements Task A's caching seam in `src/scip.rs`: `doc_cache` builds one
sorted `(start, end, occ_ix)` span vector plus a shared `LineTable` per
document, so `site_occurrence` binary-searches (`partition_point`) instead of
scanning every occurrence per site; `def_map` interns one `symbol -> (doc_ix,
occ_ix)` first-definition map per index, so `definition_of` for global symbols
is one lookup. `local ` symbols stay document-scoped per the SCIP convention.
Task B and Task C start at zero in this lane. The pre-fix walls (one process,
prior lane): ts 113 s, rust 163 s, go 436 s.

## 2. Task A: index the index (walls, RSS, output identity)

Three runs per corpus, `nice -n 15`, one process, `timeout 60`. Peak RSS by
`/usr/bin/time -l`. Ceilings: wall < 10 s AND peak RSS < 700 MB.

| lang | walls (3 runs) | peak RSS | wall ceiling | RSS ceiling |
|---|---|---:|---|---|
| ts | 3.4 / 4.9 / 3.5 s | 753 MB | pass | **miss by ~53 MB** |
| rust | 5.0 / 4.9 / 5.0 s | 974 MB | pass | **miss** |
| go | 23.9 / 24.0 / 32.8 s | 1,247 MB | **miss** | **miss** |

Output identity: `sort a.jsonl | cmp - <(sort b.jsonl)` against the
origin/main baseline binary (built in a separate worktree/target) is
IDENTICAL on all three corpora.

Two fixes landed in this lane, both in owned files:

| commit | what | effect |
|---|---|---|
| 017e23d3c + the cache-key fix | per-doc sorted span vectors + symbol->def map; deterministic path hashing (a per-call `RandomState` made every lookup miss and every site rebuild the span vector) | ts 60 s -> 3.4 s, rust 93.6 s -> 5.0 s, go 332 s -> 24 s |
| streaming decode | `scip_decode::load_index` walks the top-level protobuf fields and decodes one `Document` at a time; the whole-`Index` tree and the flat twin are never both resident (`load_index` peak alone: 692 MB -> 391 MB on the go index) | the decode is no longer the run's peak, but the run's peak is the flat index + resolve state (below) |

Where the remaining RSS lives (go informed 1,247 MB vs plain resolve
623 MB): the flat `ScipIndex` the resolve arms hold for the whole run
(952,588 occurrences, 75 MB of duplicated per-occurrence symbol strings
alone) plus the per-doc span caches and the joined-corpus contents. The next
frame is the coordinator-named one: decode once into interned u32 symbol ids
and per-doc sorted `(start, end, symbol_id)` vectors and retire the per-site
`ScipOccurrence` strings entirely; that is a seam-type change (`types.rs`,
not this lane's) and touches every resolve arm (`src/lang/*`, not this
lane's). Go's next wall frame is `types::containing_def_site` (6,783 of
~14,500 on-CPU samples in the go profile; `types.rs`, same ownership note).

## 3. Task B: go scip rows against the vta bare oracle

The 9.1% rows in SCIP.REPORT.md compared against the receiver-prefixed
`go.oracle.call.vta.tsv`; our normal form emits bare method names
(ORACLES.REPORT.md section 12). Rerun, one process, vs
`go.oracle.call.vta.bare.tsv` (55,099 rows):

| arm | rows | ∩ bare vta | coverage | precision |
|---|---:|---:|---:|---:|
| plain resolve | 92,259 | 46,517 | **84.4 %** | 50.4 % |
| scip-informed | 100,960 | 47,587 | **86.4 %** | 47.1 % |
| raw scip | 232,144 | 45,703 | 83.0 % | 19.7 % |

`out/scip_runs.sh` now carries the bench step and reads the bare oracle for
go. Plain's 84.4 % matches the #579 receipt.

## 4. Task C: informed-by-default when the index is fresh

`--resolve` with NO scip flags adopts a fresh index when one exists:
`scip_ensure::fresh_index_for_set(root, set_digest)` (new, `scip_ensure.rs`)
answers with the index whose recorded set sidecar matches the supplied file
set's digest, in v5's default cache location; `load_scip` (one hook) loads it
and emits one `tracing::info` line naming the choice, and a stale or
sidecar-less index falls back to the plain name-match leg with its own line.

- Fail-first test: `tests/scip_freshness.rs::the_informed_default_adopts_a_fresh_index_and_a_stale_one_stays_plain` (fails to compile before the fn existed, asserts the stale and no-sidecar cases stay `None`).
- Real receipt: the ts corpus with its index stamped
  (`examples/gen_scip_sidecar.rs`, machine receipt generator) — plain flags,
  wall 3 s, output byte-identical to the explicit `--scip-index` run, log
  line names the adopted path.

## 5. Receipt inventory

Committed: this report, `out/scip_runs.sh` (bare-oracle bench step),
`src/scip.rs` caches, `src/scip_decode.rs` streaming decode,
`src/scip_ensure.rs::fresh_index_for_set`, `examples/{rss_probe,gen_scip_sidecar}.rs`
(machine receipts), the freshness test. Machine-local: `out/*.raw.jsonl`,
`out/*.call.tsv`, sidecar stamps on the three corpora's indexes.

---

# Section 6+: the scip-rss lane (2026-08-30)

Lane: `fix-extract-scip-rss`, base a60e11e94 (PR #589). Ceilings per corpus,
3 runs, `nice -n 15`, `/usr/bin/time -l`, `timeout 60`: wall < 10 s AND peak
RSS < 700 MB. Baseline binary = origin/main (a60e11e94) built in a separate
target dir; identity = `sort a.jsonl | cmp - b.sorted` per corpus.

## 6. Receipt (both tasks, 3 runs per corpus)

| lang | walls real (3 runs) | peak RSS | wall ceiling | RSS ceiling | identity |
|---|---|---|---|---|---|
| ts | 1.65 / 1.66 / 1.99 s | 555-578 MB | pass | pass | IDENTICAL |
| rust | 2.24 / 2.25 / 2.37 s | 687 / 696 / 705 MB | pass | pass (borderline, one run at 705) | IDENTICAL |
| go | 7.13 / 7.13 / 8.15 s | 883-890 MB | pass | **miss by ~185 MB** | IDENTICAL |

Baseline (pre-fix binary) for comparison: ts 3.33 s / 591 MB, rust 4.89 s /
776 MB, go 22.76 s / 971 MB. Recall/precision: byte-identical jsonl on all
three corpora on every commit, so floors are unchanged by construction.

## 7. Changes, each with a before/after row

| commit | what | effect |
|---|---|---|
| af507ca25 | interned symbols: `SymbolId(u32)` into one `ScipIndex::symbols` table built at decode (`scip_decode.rs`), `ScipIndex::symbol` accessor, `ScipOccurrence`/`ScipSymbolInfo`/`ScipRelationship` carry ids, `def_map` keyed by id, per-doc span vectors hold `(start, end, SymbolId)`; the resolve arms read symbols through `ScipIndex::symbol` | go 971 -> 886 MB, rust 776 -> 700 MB, ts 591 -> 565 MB; walls unchanged |
| a2dce88da | `containing_def_site` binary search: `DefIndex` gains a per-blob span index (sorted by start, prefix max end) built once in `build_def_index`; the leftward walk prunes on the prefix max | go walls 22.4 -> 12.5 s, rust 5.0 -> 2.6 s, ts 3.3 -> 2.1 s; RSS unchanged; `containing_ts_def` rides the same index with a name exclusion |
| 267b51706 | `byte_range_cached`: the def range conversion per call site rebuilt the def document's `LineTable` every time (5,122 top-of-stack hits in the go sample); the table is now built once per buffer | go walls 12.5 -> 7.1 s, rust 2.6 -> 2.3 s, ts 2.1 -> 1.7 s; RSS unchanged |

## 8. Profile, before and after

Before (go informed, macOS `sample`, top of stack, self hits): pre-fix the
profile was dominated by `containing_def_site`'s scan (6,783 of ~14,500
on-CPU samples, the coordinator's samply count) plus `LineTable::build`
(5,122 top-of-stack hits) and `ts_tree_cursor`/parse frames. After: parse and
emit frames dominate; no scip-join function appears in the top 10
(`LineTable::build` drops out of the ranking entirely once cached).

## 9. Where the go RSS still lives, and the next frame

`load_index` on the go index now peaks at 295 MB resident (was 391 MB pre-
interning; probe: 952,588 occurrences, 137,919 symbol infos, 6.1 MB of
distinct symbol text in the interner table). The informed go run reads 883-
890 MB vs the plain leg's 623 MB, so the scip side holds ~260 MB. Measured
components:

- `ScipOccurrence` is 96 B x 952,588 = 91 MB. `override_documentation` and
  `diagnostics` are empty `Vec`s on ~99% of occurrences (48 B of the 96).
  Next frame: move both into side tables keyed by (document, occurrence) and
  the struct compacts to ~48 B -> ~46 MB back.
- `DocOccCache` span vectors: 952,588 x 12 B = 11 MB, plus per-document
  `LineTable`s kept forever in the static caches.
- The joined-corpus content buffers (`join_documents`) hold every corpus
  file's bytes for the whole resolve.
- ScipSymbolInfo 176 B x 137,919 = 23 MB plus their relationship/doc strings.

Compacting the occurrence struct is a `types.rs` + `scip_rows.rs` wire change;
it is the named next frame for the remaining ~185 MB.

## 10. Gate, ratchet, measurement notes

- Full gate `nice -n 15 cargo test --release --features cli` green (log
  `/tmp/gate.log`); targeted scip suites (golden_parity, 5_scip_facts_cli,
  n_plus_one, 32_join_documents_once, 5_move_scip, scip_freshness,
  8_scip_families_cli, 74_scip_relationship_family) 43 passed / 0 failed
  before the informed receipt runs.
- `RATCHET_BUMP=1 just extract-ratchet` run at the end (section 11).
- Measurement trap worth recording: parsing `/usr/bin/time -l` output by
  field index reads the USER column as the wall (field 3 vs field 2 on macOS)
  and shows a phantom 4x regression; every wall in this report is field 1
  (real).

