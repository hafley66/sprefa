# RUST-PARITY: two rust call oracles, one projection (lane `bench-extract-rust-parity`, 2026-08-30)

TOC
1. Corpus and inputs
2. The projection (`rust.project.py`)
3. The numbers, both oracles
4. Oracle agreement
5. Scope facts verified
6. The excess, classified
7. The ratchet port

## 1. Corpus and inputs

| input | value |
|---|---|
| corpus | `/Users/chrishafley/projects/rust-analyzer` at `af4111f`, 873 files (every `.rs` under `crates/` with a `src` component; `out/rust.files.txt`) |
| ours | one process `extract --resolve --project-root`, binary at `d1ebd8c42`, wall 8.4 s, raw 155,921 jsonl rows -> `/tmp/rp.call.tsv` 56,322 call rows (normalize.py `resolved` mode) |
| oracle A | `rust.oracle.call.tsv`, 27,004 rows, ra_ap_ide call hierarchy (`ra_ide_probe/`) |
| oracle B | `rust.scip_override.call.tsv`, 18,691 raw scip rows (SCIP.REPORT.md "raw scip 93.2%") |
| script | `plans/extract-bench-2026-08-29/rust.project.py` (stdlib only) |

## 2. The projection

`rust.project.py --ours <tsv> --oracle <tsv> --files out/rust.files.txt --scope corpus --closure enclosing`:

| flag | leg |
|---|---|
| `--scope corpus` | oracle side: drop rows whose dst_path is outside the corpus file list; ours side: drop rows whose src_path is not in the (dst-scoped) oracle's src_path set |
| `--closure enclosing` | drop a `closure@<n>` src_name row when a non-closure row shares its (src_path, dst_path, dst_name) triple, on BOTH sides (raw scip carries 3,044 closure rows; ra carries 0) |
| `--generic` | implemented, but inert on this corpus: `grep -c '<'` is 0 on `rust.oracle.call.tsv`, `rust.scip_override.call.tsv` and ours at `d1ebd8c42` |

Projection shrinkage: ra 27,004 -> 26,359 (645 oracle rows whose dst is outside the corpus); scip 18,691 -> 15,647 (scope drops 0, the closure mirror drops 3,044); ours 56,322 -> 42,738 (13,584 rows from callers the dst-scoped oracle never calls from).

## 3. The numbers (recall = overlap/|oracle|, precision = overlap/|ours|)

| oracle | projection | recall | precision | ours rows | oracle rows | overlap |
|---|---|---:|---:|---:|---:|---:|
| ra_ap_ide | raw | 68.56 | 32.87 | 56,322 | 27,004 | 18,513 |
| ra_ap_ide | scope + closure | **70.23** | **43.32** | 42,738 | 26,359 | 18,513 |
| raw scip | raw | 77.11 | 25.59 | 56,322 | 18,691 | 14,413 |
| raw scip | scope + closure | **78.75** | **42.44** | 29,031 | 15,647 | 12,322 |

The projection drops only rows outside the overlap on the ours side: the overlap with ra (18,513) is identical raw and projected, so the projection raises precision 32.87 -> 43.32 without touching recall.

## 4. Oracle agreement (ra_ap_ide vs raw scip, both projected)

| pair | a rows | b rows | overlap | a->b | b->a |
|---|---:|---:|---:|---:|---:|
| raw | 27,004 | 18,691 | 5,593 | 29.92% | 20.71% |
| projected | 26,359 | 15,647 | 5,593 | 35.74% | 34.66% |

The two oracles agree on 5,593 rows. 18,513 of our rows hit ra while only 12,322 hit scip, and their mutual overlap is 5,593, so a "parity" target above roughly 36 recall (5,593 / 15,647 scip-projected, as oracle agreement bounds it) means agreeing with an oracle choice the other oracle does not make. Cross-crate edges missing from the ra call hierarchy (STUDY.human.unga.md section 4) and scip's method-site convention are the visible causes.

## 5. Scope facts, verified

| fact | check |
|---|---|
| ~8,650 of 11,470 `ambiguous` sites target std/deps (rust.REPORT.md section 16, classes 3a/3b/5/10) | confirmed at 20.2: 6,053+ of 11,470 drops name an external receiver; no corpus row is correct for them |
| ra call hierarchy is per-crate; cross-crate edges may be missing on the oracle side | confirmed: 240 of the 300 sampled `ours - ra` excess rows are call sites whose callee def exists in the named dst file (class 1 below); `rust.oracle.call.tsv` dst coverage sits inside 26,359 corpus dst rows where scip keeps 15,647 with 5,593 agreement |
| oracle rows whose dst_path is outside `crates/*/src/**` cannot be hit | 645 of ra's 27,004 rows (2.4%); dropped by `--scope corpus` |

## 6. The excess, classified (300 rows each, seed 7, after the projection)

Classifier: `plans/extract-crawl-2026-08-29/rust.excess.classify.py`; samples committed beside it.

| class | ours - ra (of 300) | ours - scip (of 300) | example sites | emitter |
|---|---:|---:|---|---|
| valid edge, oracle miss (callee def exists in the dst file) | 240 | 260 | `crates/ide-assists/src/handlers/extract_variable.rs:131` `.syntax_element()` (extension-trait fn, `crates/syntax/src/syntax_editor.rs:455`); `crates/hir-ty/src/mir/lower.rs:632` `self.pattern_match(..)` -> `crates/hir-ty/src/mir/lower/pattern_matching.rs:66` | `src/lang/rust_receivers.rs` receiver leg |
| name pick, no such fn in the dst file (ctor/type-name/wrong-def pick by bare name) | 50 | 25 | `crates/ide/src/rename.rs:338` `Definition` variant value, dst `crates/ide-db/src/defs.rs`; `crates/hir/src/diagnostics.rs` `MalformedDerive` | `src/lang/rust.rs` bare-name corpus-unique leg |
| generated (tracked generated node/token files) | 10 | 14 | `crates/ide/src/inlay_hints/lifetime.rs:90` `for_token()` -> `crates/syntax/src/ast/generated/nodes.rs`; `crates/hir/src/semantics.rs` `pattern_adjustments` -> generated `nodes.rs` | `src/lang/rust.rs` `ast` cast/token legs |
| macro-expanded site | 0 | 1 | `crates/hir-def/src/macro_expansion_tests/mod.rs` `check_errors` -> `crates/span/src/ast_id.rs` `get_erased` | `rust_scip_macros` mint pass |
| wrong target (same name, other def, oracle picked another) | 0 in sample | 0 in sample | (detection ran; none surfaced after projection) | - |

The top class (80%+) is the ra oracle's own scope: real call sites (`x.m()` with the def textually present in the dst file) that the per-crate call hierarchy never emits. No extractor fix can score higher there; the fixable class is the 50/25 name-pick rows, under 100 excess rows in the sample and mixed ctor/type-name shapes, so it goes to a brief instead of a fail-first fix: `plans/extract-crawl-2026-08-29/rust-excess-1.FIX.BRIEF.md`.

## 7. The ratchet port

`tests/bench/mod.rs`: `RustProjection` (the `--scope corpus` ours leg against the dst-scoped oracle's src set, plus the `--closure enclosing` mirror) appended beside `GoProjection`; `Measurement.files_rel` carries the corpus file list. `tests/bench/mod.rs` `rust_projection_drops_out_of_corpus_and_mirrored_closure_rows` is the 6-row fail-first unit test. RATCHET.tsv gains `rust call rust.scip_override.call.tsv`; `tests/ratchet_recall.rs` unchanged (the projection hooks into `ratchet()` per oracle file).

| rust call row | before | after |
|---|---|---|
| vs `rust.oracle.call.tsv` | 67.56 / 33.68 (sha 5fc9ba938) | 70.23 / 43.32 (sha d1ebd8c42) |
| vs `rust.scip_override.call.tsv` (new) | - | 78.75 / 42.44 |

ts5 and go rows byte-identical. `RATCHET_FORCE=1` rust leg only, wall 555 ms, rss 617 MB.
