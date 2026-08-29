# rust — sprefa-extract corpus battery report (2026-08-28)

- [Setup](#setup)
- [Step 1 — per-file default](#step-1--per-file-default)
- [Step 2 — per-file by family](#step-2--per-file-by-family)
- [Step 3 — --resolve per crate](#step-3---resolve-per-crate)
- [Step 4 — diet_scip](#step-4--diet_scip)
- [Step 5 — scip](#step-5--scip)
- [Perf and RSS](#perf-and-rss)
- [Findings](#findings)
- [Fix landed](#fix-landed)
- [Fix landed: the large-file resource bound](#fix-landed-the-large-file-resource-bound)
- [Fix landed: the --resolve superlinear name-match](#fix-landed-the---resolve-superlinear-name-match)
- [What stays untested and why](#what-stays-untested-and-why)

## Setup

Worktree `chore/extract-corpus-rust`, merged 8e946ad. Corpus
`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/` (2506 crates). All
extract calls under `timeout 10`. Raw step-1 table: `rust.runs.tsv`.

## Step 1 — per-file default

| metric | value |
|---|---|
| files run | 77,472 |
| rc=0 | 77,246 |
| rc=1 | 225 (all `NotFound`: files deleted from the registry between listing and run; none repro on disk) |
| rc=124 (timeout) | 1: `nickel-lang-core-0.15.3/src/parser/grammar.rs` (29,328,358 B machine-generated) |
| parse errors | 0 |
| total wall ms | 1,671,625 (median ≈ 10 ms/file, process startup bound) |
| total output lines | 402,878,522 |

Macro-heavy crates (`serde_derive*`, `tokio-macros*`, `syn-*`): 957 files, all
rc=0.

## Step 2 — per-file by family

200-file sample (100 largest files + 100 random). One harness note: the
battery's line counter strips the trailing newline, so step-1 "lines" is one
below a fresh `wc -l`; corrected below.

| sum over 100 files | lines |
|---|---|
| default (all families) | 662,977 |
| cst | 580,390 |
| type | 7,469 |
| call | 13,003 |
| df | 62,215 |
| family sum | 663,077 |
| sum minus default | 100 (= 1/file, the trailing-newline counter artifact) |

Per-file, family sum equals the default run exactly. A family whose sum
exceeds the default: none.

## Step 3 — --resolve per crate

300 largest crates by file count, all `.rs` files of the crate as one project.

| metric | value |
|---|---|
| crates | 300 |
| rc=0 | 300 |
| resolved_edge | 1,086,884 |
| unresolved rows | 0 (the rust arm emits no row for ambiguous/absent callees: the documented 4b-iii discipline) |
| total wall ms | 184,226 |
| slowest crates | nickel-lang-core 27,345 ms; windows-0.61.3 17,031 ms; windows-0.62.2 15,316 ms |

Silent-drop note: an unqualified call whose callee resolves to nothing emits
no `--resolve` row at all. The per-file `site` record with `callee_path: null`
carries the fact (fixture `tests/fixtures/rust/corpus_1.rs`); finding F4.

## Step 4 — diet_scip

| metric | value |
|---|---|
| crates | 300 |
| rc=0 | 300 |
| resolved_edge | 1,086,884 (identical to step 3: same name-match leg) |
| total wall ms | 248,555 |

## Step 5 — scip

| root | result |
|---|---|
| `v6/sprefa-extract` | scip_skip: failed — the crate is a workspace member; `cargo metadata` fails outside the workspace root |
| `v6/sprefa-engine-rs` | scip_skip: same cause |
| worktree root `.` | indexed: scip_fn_edge=29,813, scip_def=8,856, scip_ref=14,024 |
| `aho-corasick-1.1.4` (copied to scratch) | indexed: scip_fn_edge=3,925 |

scip vs resolve on `aho-corasick-1.1.4`: scip_fn_edge 3,925 vs resolved_edge
1,549. Name formats differ (qualified scip symbols vs bare names), so the
edge sets were compared by callee name tail. Sample of 20 scip-only edges,
classified:

| class | count | examples |
|---|---|---|
| method call on typed receiver (scip knows the type) | 8 | `Automaton#is_match()` |
| type/const/variant constructor refs (outside the call facet) | 6 | `Anchored#Yes#`, `REGRESSION.` |
| trait-method through impl and dyn | 3 | `Arc<dyn AcAutomaton>::patterns_len` |
| macro site | 1 | `testconfig!` |
| inherent impl same-file (scip splits impl blocks) | 2 | `Input<'h>::haystack()` |

Resolve-only edges (787) are method calls on receivers whose type is not
nameable by the parse (all `is_match`/`follow_transition` impl-site matches)
plus std (`try_from`, `alloc_transition`) and closure/field callees.

## Perf and RSS

- bytes/ms 5th percentile: 19.46 (`serde_with_macros-3.21.0/tests/serde_as_issue_267.rs`, 253 B / 13 ms). Sub-percentile rows are 1-10 B files where process startup dominates: no construct is slow.
- The slow tail is machine-generated tables, not a language construct: nickel grammar.rs (20.8 s unbounded, 13,664,266 output lines), chrono-tz `timezones.rs` (7.2 MB), windows `mod.rs` batch.
- RSS, 20 largest files (`rust.rss_top20.tsv`): peak 3,762,643,968 B (3.74 GB) on nickel `grammar.rs` (128x input size); 824 MB on chrono-tz; 200-500 MB on the windows `mod.rs` set.

## Findings

| lang | class | path:line | repro command | observed | expected |
|---|---|---|---|---|---|
| rust | timeout | nickel-lang-core-0.15.3/src/parser/grammar.rs | `timeout 10 extract <grammar.rs>` | rc=124; unbounded 20.8 s, 13.6M lines | completes or streams bounded |
| rust | rss | nickel-lang-core-0.15.3/src/parser/grammar.rs | `/usr/bin/time -l extract <grammar.rs>` | peak RSS 3,762,643,968 B | bounded footprint |
| rust | wrong_fact (FIXED) | v6/sprefa-extract/src/lang/rust.rs `call_name_match` | `cargo test --features cli --test 60_rust_corpus_scope` | same-file call bound to a foreign file's identical `(name, span)` def (12_rust_scope_helper_a.rs) | same-file def wins |
| rust | missing_fact | `--resolve` pipeline, `CallF` resolve | `extract --family call tests/fixtures/rust/corpus_1.rs` vs `--resolve` with a second file | unresolved call site emits no `--resolve` row (by-design 4b-iii); only per-file `site` with `callee_path: null` carries it | a visible unresolved marker in project mode, or a documented flag |

## Fix landed

- Failing test FIRST: `tests/60_rust_corpus_scope.rs` red (observed edge
  `callee_path: 12_rust_scope_helper_a.rs`), then the fix in
  `src/lang/rust.rs` (`own_file_blob` file-fingerprint join over the whole
  named-def span set of the file), then green.
- Whole crate after the fix: `cargo test --features cli` 354 passed, 0 failed,
  2 ignored. aho-corasick `--resolve` count unchanged (1,549 edges).

## What stays untested and why

- `include!`-expanded sources: no registry crate ships the included files
  outside `build.rs` codegen; not reproducible without cargo build.
- `#[cfg]`-gated modules under their enabled cfgs: needs a target's
  feature set; phase-1 is cfg-blind by design and every cfg form parsed rc=0
  in step 1.
- `mod.rs` vs `foo.rs` scope owner on the real corpus: covered by
  `tests/30_rust_mod_scope_owner.rs` fixtures; no registry-only variant adds a
  shape the fixtures lack.
- scip on the two v6 crate roots directly: impossible until they are
  standalone packages (workspace-membership failure, recorded above); the
  worktree root substitutes.
- Trait-impl method callees under `--resolve`: name-only by the 4a ADDENDUM
  (receiver typing out of scope); scip is the oracle for those edges (step 5).

## Fix landed: the large-file resource bound

Lane `fix-extract-large-files`, base sha `99b8dc79f`, commit "extract: stream
flat facts instead of collecting them". Full write-up and the byte-identity
receipt: `ts.REPORT.md` section 12.

| finding | before | after | test |
|---|---|---|---|
| `rss` on `nickel-lang-core-0.15.3/src/parser/grammar.rs` (29,328,358 B) | 3,610,509,312 B peak RSS | 3,004,694,528 B | `tests/9_large_file_bounds.rs::rs_all_families_rss_is_bounded` |
| wall, same file, no `timeout` | 14.24 s | 12.55 s | (same) |
| `timeout` rc=124 under `timeout 10`, same file | rc=124, empty stream | rc=0, one `size_skip` row | `tests/9_size_skip.rs`, 8 cases |

Output on that file is byte-identical before and after: 13,664,266 rows,
`cmp -s` clean.

### The timeout is parse time, not row time

`extract --bench --family <f>` on the same file splits extract from flatten:

| family | extract | serial (flatten) | rows | peak RSS |
|---|---|---|---|---|
| cst | 7.043 s | 224.8 ms | 11,416,699 | 2,039,300,096 B |
| type | 5.119 s | 6.9 ms | 175,430 | 2,419,736,576 B |
| call | 5.494 s | 4.0 ms | 129,247 | 2,292,367,360 B |
| df | 4.475 s | 71.3 ms | 1,942,890 | 2,876,817,408 B |
| data | (0.02 s wall) | | 0 | 34,684,928 B |
| default (all) | | | 13,664,266 | 3,004,694,528 B, 12.55 s |

Flatten is 0.1% to 3% of each family's wall. The 12.55 s is the ast-grep parse
(7.0 s) plus the one shared syn parse and its three projections. No change to
the row plane moves it, and the 3.0 GB floor is the syn AST: `--family type`
peaks at 2.42 GB while emitting 175,430 rows.

The corpus gap is sharp. Second-slowest rust file is `chrono-tz` `timezones.rs`
at 3,789 ms for 7.2 MB; nickel `grammar.rs` is 29.3 MB. One file in 77,472
exceeds 10 s.

### Named size skip: LANDED

Ownership for `src/types.rs` and `src/schema.rs` was extended by the
coordinator for this record only. The row:

```
record=size_skip  path=<string>  bytes=<u64>  limit=<u64>  reason=<over_max_bytes>
```

An input over the ceiling is not parsed: `extract` emits that one row and exits
0. `--max-bytes N` sets the ceiling, `--max-bytes 0` removes it, default
16,777,216 B. The decision is made on file size before any parse, so it covers
the normal family stream, `--bench` and `--ast-pattern` alike. `--file-fact`
still prepends its identity row: a digest and a line count over bytes already
read is not the cost being bounded. A whole-project mode (`--resolve`,
`--deps`, `--scip-*`) takes directories and path sets and is not covered.

The finding's exact repro, before and after:

| command | before | after |
|---|---|---|
| `timeout 10 extract <nickel grammar.rs>` | rc=124, empty stream, 10 s burned | rc=0, one `size_skip` row, milliseconds |
| `timeout 10 extract --max-bytes 0 <same>` | (no such flag) | rc=124, unchanged; the unbounded path is still reachable |

```json
{"record":"size_skip","path":".../nickel-lang-core-0.15.3/src/parser/grammar.rs","bytes":29328358,"limit":16777216,"reason":"over_max_bytes"}
```

### The ceiling is measured, not chosen

| corpus | files over 16,777,216 B |
|---|---|
| rust registry, 77,472 files | 1: `nickel-lang-core-0.15.3/src/parser/grammar.rs` |
| ts/js (`~/projects/instant`) | 0 |
| this crate's `tests/fixtures/**` | 0 (largest is a 1 MB golden jsonl, not an input) |

So the default changes no existing stream. Verified: the 19 corpus files of the
20-file parity sample that sit under the ceiling stay byte-identical against the
pre-fix binary; the 20th is the nickel file, whose output is the skip row by
design.

One existing test needed the escape hatch, not a weakening.
`tests/45_emit_throughput.rs` builds a 25,563,904 B synthetic `.go` to measure
JSONL emission on 350k rows, which is over the ceiling by construction. It now
passes `--max-bytes 0`; its budget, its row-count equality assert and its input
are unchanged. It is the only test in the crate that generates an over-ceiling
input (`27_blob_cache.rs` writes 4 KB, `46_resolve_scaling.rs` runs
`--resolve`, which the ceiling does not cover).

Tests: `tests/9_size_skip.rs`, 8 cases. Over-ceiling emits exactly one row with
the right path, bytes and limit at rc=0; the boundary is inclusive (a file at
its own ceiling extracts); `--max-bytes` lowers it; `--max-bytes 0` disables it;
`--file-fact` still rides a skip; an under-ceiling file is unchanged;
`--ast-pattern` skips too; `--schema` declares the record.

## Fix landed: the --resolve superlinear name-match

Lane `fix-extract-rust-resolve-perf`, base sha `cec3d5c1d`.

`own_file_blob` (`src/lang/rust.rs`) learned the file's own `ContentId` once per
CALL SITE and scanned the whole corpus `DefIndex` to do it, so the name-match
cost grew as sites x index entries x candidate blobs. It is now computed once
per FILE, threaded into the new `RustSource::call_name_match_in`, and seeded on
the file's rarest def name so a corpus-wide name like `helper` or `new` no
longer puts every file in the candidate set.

Corpus: `tokio-1.48.0` from the crates.io registry, first N `.rs` files by
`find <dir> -name '*.rs' | sort`, `extract --resolve <files>`, release binary,
median of 3 interleaved runs.

| finding | before | after | test |
|---|---|---|---|
| `perf`, wall at 200 files | 260 ms | 100 ms | `tests/49_rust_resolve_scaling.rs::rust_resolve_wall_grows_linearly_with_file_count` |
| `perf`, wall at 400 files | 1,300 ms | 180 ms | (same) |
| `perf`, ratio 400/200 | 5.0x | 1.8x | (same, budget 2.5x) |
| `perf`, own-blob probes at 50 synthetic files | 25,349,600 | 100 | `tests/49_rust_resolve_scaling.rs::own_blob_probes_stay_linear_in_the_file_count` |
| `perf`, own-blob probes at 400 synthetic files | did not terminate in a usable time | 800 | (same, bound 4/file) |
| `timeout`, `rust-analyzer/crates/syntax` (58 files; `nodes.rs` carries 2,508 defs and 3,320 sites) | rc=124 under `timeout 10`; 19.16 s untimed | rc=0, 290 ms | `tests/49_rust_resolve_scaling.rs::a_generated_node_file_resolves_under_the_ten_second_law` |

Output is byte-identical before and after on the 400-file tokio corpus (6,274
rows) and on `rust-analyzer/crates/syntax` (4,432 rows), `cmp -s` clean both
times. The same-file-wins edge of `tests/60_rust_corpus_scope.rs` is re-asserted
inside the new test file.

Whole crate after the fix: `cargo test --features cli --no-fail-fast` rc=0,
392 passed, 0 failed, 2 ignored.
