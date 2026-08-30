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
