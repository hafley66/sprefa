# SCIP.REPORT.md — how far scip gets us past codeql, one process per corpus (2026-08-29)

Lane: `bench-extract-scip-informed`. Binary: `v6/sprefa-extract/target/release/extract`
(worktree `bench/extract-scip-informed`, base 8898448a1).

## Contents

1. Indexes used, walls, samples
2. Task A: recall tables (plain / scip-informed / raw scip / codeql2 / joern2)
3. The ratio the user asked for: raw scip rows vs consumed vs plain
4. Task B: the gap between raw scip and our consumption
5. Task C: the fix inside the scip seam
6. Defects found, what stays open

## 1. Indexes and walls

Indexes stay out of git; these are their machine paths:

| lang | index path | tool |
|---|---|---|
| ts | `/Users/chrishafley/projects/TypeScript-5.9/src/.dl/.state/index.scip` | scip-typescript |
| go | `/Users/chrishafley/projects/typescript-go/index.scip` | scip-go (built this lane, 900 s budget) |
| rust | `/Users/chrishafley/projects/rust-analyzer/.dl/.state/index.scip` | rust-analyzer |

Walls, one process, whole corpus (`timeout` was raised above the 60 s law to GET
the data; the 60 s ceiling would kill every scip-informed run):

| lang | plain resolve | scip-informed resolve | raw scip (`--family scip`) |
|---|---:|---:|---:|
| ts (600 files) | 2.0 s | 113 s | 1 s (cached) |
| rust (873 files) | 1.4 s | 163 s | 2 s (cached) |
| go (5,087 files) | 11.3 s | 436 s | 3 s (cached) |

5 s `sample` top frames where the wall is over 10 s:

| lang | hot frames (samples) |
|---|---|
| ts | `scip::site_occurrence` (1820+1759 of 3940), `scip::definition_of` (101) |
| rust | `scip::definition_of` (484 of 3524), `scip::site_occurrence` (14), `byte_range_at` (11+6) |

`definition_of` scans every document's occurrence list per call site for a
global symbol; `site_occurrence` builds a `LineTable` per (doc, site) pair.
Both live in `src/scip.rs` and are the seam's own cost.

## 2. Task A: recall tables

Call family, normal-form tsvs in `out/`, comparisons via `bench.py`.
"coverage" = |a ∩ oracle| / |oracle| (the user's 84.88 % ts convention);
"precision" = |a ∩ oracle| / |a|. Oracle in scope of each corpus's file list.

### ts (oracle = TypeChecker, `ts5.oracle.call.tsv`, 59,356 rows)

| tool | rows | ∩ oracle | coverage | precision |
|---|---:|---:|---:|---:|
| plain resolve (`ts.plain.call.tsv`) | 70,799 | 50,383 | **84.88 %** | 71.2 % |
| scip-informed (`ts.scipinformed.call.tsv`) | 75,400 | 52,614 | **88.64 %** | 69.8 % |
| raw scip (`ts.rawscip.call.tsv`) | 83,361 | 14,715 | 24.8 % | 17.7 % |
| codeql2 (`ts.codeql2.call.tsv`, 53,140) | | 47,101 / 48,614 / 13,730 ∩ | | |
| joern2 (`ts.joern2.call.tsv`, 24,451) | | 18,147 / 18,106 / 8,138 ∩ | | |

scip-informed BEATS codeql2 coverage (52,614 vs 48,614) and the user's 88.6 %
target. Raw scip's low overlap is a naming artifact: `scip_fn_edge` rows carry
every reference inside the enclosing callable (type names, consts), not just
calls (section 4).

### rust (oracle = ra_ap_ide call hierarchy, `rust.oracle.call.tsv`, 26,284 rows in corpus scope)

| tool | rows | ∩ oracle | coverage | precision |
|---|---:|---:|---:|---:|
| plain resolve | 56,048 | 18,296 | 69.6 % | 32.6 % |
| scip-informed | 73,565 | 23,137 | **87.95 %** | 31.4 % |
| raw scip | 149,045 | 24,487 | 93.2 % | 16.4 % |
| codeql2 / joern2 | | not built for rust | | |

scip-informed moves rust +18.3 coverage points over plain. Raw scip's ceiling
is 93.2 %; we consume 23,137 of its 24,487 oracle-hitting rows (94.5 %).

### go (oracle = vta, `go.oracle.call.vta.tsv`, 58,332 rows in corpus scope)

| tool | rows | ∩ vta | coverage | precision |
|---|---:|---:|---:|---:|
| plain resolve | 92,259 | 5,303 | 9.1 % | 5.7 % |
| scip-informed | 100,960 | 5,314 | 9.1 % | 5.3 % |
| raw scip | 232,144 | 5,320 | 9.1 % | 2.3 % |
| codeql2 (48,529) | | 47,244 / 48,448 / 48,407 ∩ | | |
| joern2 (31,617) | | 26,461 / 26,672 / 26,546 ∩ | | |

Against `go.ctl.call.tsv` (23,837): plain 23,625 ∩, informed 23,526 ∩. The vta/
cha overlap is small because vta/cha are whole-program over-approximations and
both tools answer a different question at that end (ORACLES.REPORT.md §7,
finding 3). Scip moves go by +11 vta-hitting rows; go's informed leg adds
nothing the name-match did not already have.

### type family

| lang | plain | informed | raw scip (scip_impl, normal form) | oracle |
|---|---:|---:|---:|---|
| ts | 20,317 | 20,317 (identical) | 190 | none committed (`ts5.oracle.type.tsv` absent) |
| rust | 2,954 | 2,954 (identical), ∩ `rust.oracle.type.tsv` 2,192 / 43,134 | 0 (ra emits no impl rels over crates/**) | 2,192 coverage |
| go | 4,641 | 4,641 (identical) | 2,098 | no type oracle committed |

The type arm is untouched by scip in all three arms today.

## 3. The ratio: raw scip rows vs consumed vs plain

| lang | family | raw scip rows | consumed from scip (`scip_override` + `scip_macro`) | plain resolve rows | raw scip `scip_fn_edge` |
|---|---|---:|---:|---:|---:|
| ts | call | 83,361 | 6,121 | 70,799 | 90,962 |
| ts | type | 190 | 0 | 20,317 | |
| rust | call | 149,045 | 19,521 + 3,687 macro | 56,048 | 173,502 |
| rust | type | 0 | 0 | 2,954 | |
| go | call | 232,144 | 10,983 | 92,259 | 243,769 |
| go | type | 2,098 | 0 | 4,641 | |

Consumed / raw `scip_fn_edge`: ts 6.7 %, rust 13.3 %, go 4.5 %.
Chunked-driver comparison (ORACLES.REPORT.md): ts 3.1 %, rust 0.3 %. The
single-process run consumes 4-40x more of scip because every cross-crate site
is present.

## 4. Task B: the gap, (raw scip ∩ oracle) − scip-informed

| lang | gap rows | sample (seed 7) |
|---|---:|---:|
| ts | 152 | 152 classified |
| rust | 1,610 in corpus scope | 300 classified |
| go | 6 | not sampled |

Classification of 300 (rust) + 152 (ts), classes per the brief, with the
`src/*.rs` fn that owns each class:

| class | rust | ts | seam fn that would take it | ownership |
|---|---:|---:|---|---|
| A: no phase-1 call site (site span absent; the parse never makes the callee expression a `CallSite`) | 240 | 119 | none: the site list is `call.aux.sites` from the parse | lang arms (NOT this lane) |
| E: site exists, def exists; drop after the def join | 51 | 33 | `types.rs::containing_def_site` / caller attribution (`covering_def` picks `closure@N`, the oracle the enclosing named fn) | types.rs (NOT this lane) |
| C: trait-impl method (dynamic dispatch), `impl#[...]` symbols | 9 | 0 | `scip_v5_rels::v5_rel_rows` emits the data; an arm would consume it | seam (this lane) + arms |
| B: symbol not mapped to a def | 0 | 0 | `scip::definition_of` (measured: every gap callee has a def) | seam |
| D: cross-file symbol with no `ContentId` | 0 | 0 | `scip::join_documents` | seam |
| other | 0 | 0 | | |

So the top class is NOT seam-reachable: scip cannot correct a call the parser
never saw (rust enum-variant / tuple-struct constructor calls written
`Path::Variant(...)`, `LazyProperty::Computed(edit)`; ts named calls that
phase-1 never emitted, e.g. `getEmitModuleKind` at
`factory/utilities.ts:699` has no phase-1 site while its neighbours do). The
dominant rust class A examples are all variant-ctor calls; the oracle (ra_ide)
counts them as calls, scip resolves them, and no site exists to attach the
answer to.

## 5. Task C: the fix inside the scip seam

Class C was the top seam-reachable class. Fix: `--family scip` now emits the
raw `scip_relationship` rows instead of flattening every relationship into
`scip_impl` (scip_v5_rels.rs). Before: 0 rows. After, one process per corpus:

| lang | scip_impl | scip_relationship (after) |
|---|---:|---:|
| ts | 877 | 877 |
| go | 4,763 | 4,763 |
| rust | 0 | 0 (ra's scip build emits no relationships over crates/**) |

The override-pair flags (`is_reference` + `is_implementation`) now ride the
wire, which is what a dispatch arm needs to bind a trait-method call site to
its impls.

- Fail-first test: `tests/7N_scip_relationship_family.rs` against the committed
  2.6 KB fixture `tests/fixtures/scip_relationship/fixture.scip` (scip_move
  precedent for a committed index with machine-local baked paths; README
  documents the rebuild). It failed before the fix ("--family scip emitted no
  scip_relationship rows"), passes after.
- Golden: `tests/fixtures/scip_families/scip_rel.jsonl` regenerated (adds the
  two relationship rows); vocabulary comment updated in
  `tests/8_scip_families_cli.rs`.
- Recall receipt: the type-family and call-family recall rows are unchanged
  (no resolve arm consumes the new rows; the arms belong to three live lanes).
  The row is now on the wire for the arm lanes to consume.

Gate: `cargo test --features cli` green except `tests/6_kind_vocab.rs::wire_output_is_byte_identical_to_the_946460d75_golden`, which fails identically with the change stashed (pre-existing drift, +3,100 bytes, not this lane's).

## 6. Defects found, what stays open

| finding | evidence |
|---|---|
| `--resolve --scip-index` hard-fails when the index carries documents outside the project root | scip-go indexes 65 `../../Library/Caches/go-build/...` test-binary docs; `SourceTreeBlobSource::open_files` (project.rs:1343) returns Err from soopy's path check and the whole run dies rc=1. Worked around by pruning the 65 docs from the go index protobuf (`index.scip.full` kept beside it). A seam-side skip-unreadable-docs is the fix; project.rs is not this lane's. |
| go informed adds +11 oracle rows for +8,701 rows | the scip go leg corrects sites the name-match already had right; net new truth is marginal (section 2). |
| `scip_fn_edge` is a reference graph, not a call graph | 83,361 ts rows vs 59,356 oracle, 17.7 % precision. Any consumer treating it as calls gets type refs and consts as callees. |
| ts phase-1 site gaps | `getEmitModuleKind` called at `factory/utilities.ts:699` has no phase-1 site (plain and informed both), while the adjacent imported call at line 700 does. Class A, lang-arm lane. |
| `definition_of` / `site_occurrence` are the informed-leg cost | 163 s rust / 113 s ts for 873/600 files; per-site full-document scans (section 1). A per-index symbol->def map in the seam would need `ScipIndex` mutable state (types.rs) or an index-keyed cache; not attempted this lane. |

## 7. Receipt inventory

Committed: this report, `out/scip_runs.sh`, `out/{lang}.files.txt`,
`out/{lang}.gap.tsv`. Machine-local (over the 1 MB git law, `.gitignore` in
`out/`): `out/{lang}.{plain,scipinformed,rawscip}.{call,type}.tsv` and the raw
`out/*.raw.jsonl` streams. `go/index.scip.full` (unpruned go index, 106 MB) and
`go/index.scip` (65 escaping-doc entries pruned) stay beside the corpus.
Regenerate everything with `out/scip_runs.sh` plus `normalize.py` /
`bench.py` in this directory.
